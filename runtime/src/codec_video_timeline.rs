//! Native presentation-time comparison; the frozen lookup digest never proves video equality.
use crate::codec_error::CodecError;
use crate::codec_video::VideoDecoder;
use crate::codec_video_error::{CommandContext, VideoError, VideoFailure, capture_result};
use std::path::Path;
use std::sync::Arc;

pub const FRAME_BYTES: usize = 64 * 64 * 4;
const MAX_FRAMES: usize = 128;

#[derive(Clone)]
pub struct Timeline {
    pub rgba: Arc<Vec<u8>>,
    pub times: Vec<f64>,
    pub duration: f64,
}

#[derive(Clone, Copy)]
pub(crate) enum Policy {
    Legacy,
    Source,
}

pub(crate) fn unknown(message: impl Into<String>) -> VideoError {
    VideoError {
        error: CodecError::unverified(message),
        caught_by_exception: false,
    }
}

impl Policy {
    fn semantic(self, path: &Path, message: &str) -> VideoError {
        match self {
            Self::Legacy => unknown(message),
            Self::Source => VideoError {
                error: CodecError {
                    kind: "UndecodableVideo".into(),
                    message: format!(
                        "{}: {message}",
                        path.file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy()
                    ),
                    handled: true,
                },
                caught_by_exception: true,
            },
        }
    }

    fn failure(self, path: &Path, error: VideoFailure) -> VideoError {
        match self {
            Self::Legacy => unknown(error.legacy()),
            Self::Source => error.classified(path),
        }
    }

    fn duration(self, path: &Path, duration: f64) -> Result<(), VideoError> {
        if !duration.is_finite() || duration <= 0.0 || duration > 3.05 {
            Err(self.semantic(
                path,
                "complete video duration is outside the comparison limit",
            ))
        } else {
            Ok(())
        }
    }
}

pub fn parse(raw: Vec<u8>, stderr: &[u8], duration: f64) -> Result<Timeline, String> {
    parse_with(Path::new(""), raw, stderr, duration, Policy::Legacy)
        .map_err(|error| error.error.message)
}

pub fn parse_typed(
    path: &Path,
    raw: Vec<u8>,
    stderr: &[u8],
    duration: f64,
) -> Result<Timeline, CodecError> {
    parse_with(path, raw, stderr, duration, Policy::Source).map_err(|error| error.error)
}

fn parse_with(
    path: &Path,
    raw: Vec<u8>,
    stderr: &[u8],
    duration: f64,
    policy: Policy,
) -> Result<Timeline, VideoError> {
    if matches!(policy, Policy::Source) {
        policy.duration(path, duration)?;
    }
    let mut times = timestamps(stderr)
        .into_iter()
        .map(|token| match policy {
            Policy::Legacy => std::str::from_utf8(token)
                .ok()
                .and_then(|text| text.parse::<f64>().ok())
                .ok_or_else(|| unknown("invalid native frame timestamp")),
            Policy::Source => token
                .is_ascii()
                .then(|| std::str::from_utf8(token).ok())
                .flatten()
                .and_then(crate::codec_video_probe::float_text)
                .ok_or_else(|| {
                    VideoError::known(CodecError::value(format!(
                        "could not convert string to float: {}",
                        bytes_repr(token)
                    )))
                }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = raw.len() / FRAME_BYTES;
    if !raw.len().is_multiple_of(FRAME_BYTES)
        || count == 0
        || count > MAX_FRAMES
        || times.len() != count
    {
        return Err(policy.semantic(path, "native frames and timestamps are incomplete"));
    }
    if (matches!(policy, Policy::Legacy)
        && (!duration.is_finite() || duration <= 0.0 || duration > 3.05))
        || times.iter().any(|value| !value.is_finite())
        || times.windows(2).any(|pair| pair[1] <= pair[0])
    {
        return Err(policy.semantic(path, "native frame timestamps cannot be aligned"));
    }
    let origin = times[0];
    let duration = duration - origin;
    for time in &mut times {
        *time -= origin;
    }
    let tail = duration - times[count - 1];
    if !(tail > 0.0 && tail <= 3.0) {
        return Err(policy.semantic(path, "final frame duration cannot be established"));
    }
    Ok(Timeline {
        rgba: Arc::new(raw),
        times,
        duration,
    })
}

pub(crate) async fn decode(
    decoder: &VideoDecoder,
    path: &Path,
    policy: Policy,
) -> Result<Timeline, VideoError> {
    let duration = decoder
        .probe_core(path, CommandContext::Direct)
        .await
        .map_err(unknown)?
        .map_err(|error| policy.failure(path, error))?
        .duration;
    policy.duration(path, duration)?;
    let mut args = vec!["-nostdin".into(), "-v".into(), "info".into()];
    args.extend(
        decoder
            .decoder_args_core(path, policy)
            .await
            .map_err(unknown)?
            .map_err(|error| policy.failure(path, error))?,
    );
    args.extend([
        "-i".into(),
        path.to_string_lossy().into_owned(),
        "-map".into(),
        "0:v:0".into(),
        "-an".into(),
        "-threads".into(),
        "1".into(),
        "-filter_threads".into(),
        "1".into(),
        "-vf".into(),
        "scale=64:64,format=rgba,showinfo".into(),
        "-fps_mode".into(),
        "passthrough".into(),
        "-frames:v".into(),
        (MAX_FRAMES + 1).to_string(),
        "-f".into(),
        "rawvideo".into(),
        "-".into(),
    ]);
    let settled = crate::codec_process::capture_outcome_input(
        &decoder.root,
        &decoder.ffmpeg,
        &args,
        ((MAX_FRAMES + 1) * FRAME_BYTES) as u64,
        None,
    )
    .await
    .map_err(unknown)?;
    let result = capture_result(&decoder.ffmpeg, &args, CommandContext::Direct, settled)
        .map_err(|error| policy.failure(path, error))?;
    parse_with(path, result.stdout, &result.stderr, duration, policy)
}

fn frame(timeline: &Timeline, index: usize) -> Option<&[u8]> {
    timeline
        .rgba
        .get(index.checked_mul(FRAME_BYTES)?..(index + 1).checked_mul(FRAME_BYTES)?)
}

fn at(times: &[f64], midpoint: f64) -> usize {
    times
        .partition_point(|&time| time <= midpoint)
        .saturating_sub(1)
}

pub fn same_image(left: &Timeline, right: &Timeline) -> Option<bool> {
    compare(left, right, Policy::Legacy).ok().flatten()
}

fn compare_frame(x: &[u8], y: &[u8], policy: Policy) -> Result<Option<bool>, VideoError> {
    match crate::native_raster::same_frame(x, y) {
        Some(same) => Ok(Some(same)),
        None if matches!(policy, Policy::Legacy) => Ok(None),
        None => Err(unknown("native comparison raster is incomplete")),
    }
}

pub(crate) fn compare(
    left: &Timeline,
    right: &Timeline,
    policy: Policy,
) -> Result<Option<bool>, VideoError> {
    // Public callers can construct a Timeline, so reject incomplete evidence here too.
    for timeline in [left, right] {
        if timeline.times.is_empty()
            || timeline.times.len() > MAX_FRAMES
            || timeline.rgba.len() != timeline.times.len() * FRAME_BYTES
            || !timeline.duration.is_finite()
            || timeline.duration <= 0.0
            || timeline.times[0] != 0.0
            || timeline.times.iter().any(|time| !time.is_finite())
            || timeline.times.windows(2).any(|pair| pair[1] <= pair[0])
            || timeline
                .times
                .last()
                .is_some_and(|time| *time >= timeline.duration)
        {
            return match policy {
                Policy::Legacy => Ok(None),
                Policy::Source => Err(unknown("native comparison timeline is incomplete")),
            };
        }
    }
    if (left.duration - right.duration).abs() > 1.0 / 30.0 + 0.002 {
        return Ok(Some(false));
    }
    for (own, other) in [(left, right), (right, left)] {
        for (index, &start) in own.times.iter().enumerate() {
            let end = own.times.get(index + 1).copied().unwrap_or(own.duration);
            let matched = at(&other.times, (start + end) / 2.0);
            let (Some(left), Some(right)) = (frame(own, index), frame(other, matched)) else {
                return Err(unknown("native comparison frame is incomplete"));
            };
            match compare_frame(left, right, policy)? {
                Some(false) => return Ok(Some(false)),
                None => return Ok(None),
                Some(true) => {}
            }
        }
    }
    let stop = left.duration.min(right.duration);
    let mut edges: Vec<f64> = left
        .times
        .iter()
        .chain(&right.times)
        .copied()
        .filter(|&time| time < stop)
        .collect();
    edges.push(stop);
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    for pair in edges.windows(2) {
        let [start, end] = [pair[0], pair[1]];
        let midpoint = (start + end) / 2.0;
        let (Some(x), Some(y)) = (
            frame(left, at(&left.times, midpoint)),
            frame(right, at(&right.times, midpoint)),
        ) else {
            return Err(unknown("native comparison frame is incomplete"));
        };
        match compare_frame(x, y, policy)? {
            Some(false) => {
                let rounded = end - start <= 0.002 + 1e-9
                    && ((left.times[1..].contains(&start) && right.times[1..].contains(&end))
                        || (right.times[1..].contains(&start) && left.times[1..].contains(&end)));
                if !rounded {
                    return Ok(Some(false));
                }
            }
            None => return Ok(None),
            Some(true) => {}
        }
    }
    Ok(Some(true))
}

// The byte matcher has the exact ASCII whitespace/digit/word-boundary grammar used by showinfo.
fn timestamps(stderr: &[u8]) -> Vec<&[u8]> {
    fn word(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }
    fn whitespace(byte: u8) -> bool {
        matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12)
    }
    fn spaces(bytes: &[u8], position: &mut usize) -> usize {
        let start = *position;
        while bytes.get(*position).is_some_and(|&b| whitespace(b)) {
            *position += 1;
        }
        *position - start
    }
    let mut tokens = Vec::new();
    let mut start = 0;
    while start + 2 <= stderr.len() {
        if stderr.get(start..start + 2) != Some(b"n:") || (start > 0 && word(stderr[start - 1])) {
            start += 1;
            continue;
        }
        let mut pos = start + 2;
        spaces(stderr, &mut pos);
        let digits = pos;
        while stderr.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        if pos == digits
            || spaces(stderr, &mut pos) == 0
            || stderr.get(pos..pos + 4) != Some(b"pts:")
        {
            start += 1;
            continue;
        }
        pos += 4;
        spaces(stderr, &mut pos);
        let pts = pos;
        while stderr.get(pos).is_some_and(|&b| !whitespace(b)) {
            pos += 1;
        }
        if pos == pts
            || spaces(stderr, &mut pos) == 0
            || stderr.get(pos..pos + 9) != Some(b"pts_time:")
        {
            start += 1;
            continue;
        }
        pos += 9;
        let token = pos;
        while stderr.get(pos).is_some_and(|&b| !whitespace(b)) {
            pos += 1;
        }
        if token < pos {
            tokens.push(&stderr[token..pos]);
            start = pos;
        } else {
            start += 1;
        }
    }
    tokens
}
fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut repr = String::from("b");
    repr.push(quote as char);
    for &byte in bytes {
        match byte {
            b'\n' => repr.push_str("\\n"),
            b'\r' => repr.push_str("\\r"),
            b'\t' => repr.push_str("\\t"),
            b'\\' => repr.push_str("\\\\"),
            byte if byte == quote => {
                repr.push('\\');
                repr.push(byte as char);
            }
            32..=126 => repr.push(byte as char),
            byte => repr.push_str(&format!("\\x{byte:02x}")),
        }
    }
    repr.push(quote as char);
    repr
}
