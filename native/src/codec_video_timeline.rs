//! Native presentation-time comparison; the frozen lookup digest never proves video equality.
use crate::codec_error::CodecError;
use crate::codec_video::VideoDecoder;
use crate::codec_video_error::{CommandContext, VideoError, VideoFailure, capture_result};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
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
    Python::initialize();
    let mut times = Python::attach(|py| -> Result<Vec<f64>, VideoError> {
        // Keep the source byte regex and float(bytes) grammar; metadata outside tokens is opaque.
        let tokens = py
            .import("re")
            .and_then(|module| {
                module.call_method1(
                    "findall",
                    (
                        PyBytes::new(py, br"\bn:\s*\d+\s+pts:\s*\S+\s+pts_time:([^\s]+)"),
                        PyBytes::new(py, stderr),
                    ),
                )
            })
            .and_then(|value| value.extract::<Vec<Vec<u8>>>())
            .map_err(|error| VideoError::from_python(py, error))?;
        tokens
            .into_iter()
            .map(|token| match policy {
                Policy::Legacy => std::str::from_utf8(&token)
                    .ok()
                    .and_then(|text| text.parse::<f64>().ok())
                    .ok_or_else(|| unknown("invalid native frame timestamp")),
                Policy::Source => py
                    .import("builtins")
                    .and_then(|module| module.getattr("float"))
                    .and_then(|float| float.call1((PyBytes::new(py, &token),)))
                    .and_then(|value| value.extract::<f64>())
                    .map_err(|error| VideoError::from_python(py, error)),
            })
            .collect()
    })?;
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
    match crate::codec_raster::same_frame_external(x, y) {
        Ok(Some(same)) => Ok(Some(same)),
        Ok(None) if matches!(policy, Policy::Legacy) => Ok(None),
        Ok(None) => Err(unknown("native comparison raster is incomplete")),
        Err(_) if matches!(policy, Policy::Legacy) => Ok(None),
        Err(error) => Err(Python::attach(|py| VideoError::from_python(py, error))),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_timeline_parsing_keeps_source_float_bytes_and_semantic_error_identity() {
        let source = Path::new("sample.webm");
        let timeline = parse_typed(
            source,
            vec![0; FRAME_BYTES],
            b"n: 0 pts: 0 pts_time:0_0.0 ",
            1.0,
        )
        .unwrap();
        assert_eq!(timeline.times, [0.0]);
        assert_eq!(timeline.duration, 1.0);
        for (raw, stderr, duration, kind, message, handled) in [
            (
                vec![0; FRAME_BYTES],
                b"n: 0 pts: 0 pts_time:bad ".as_slice(),
                1.0,
                "ValueError",
                "could not convert string to float: b'bad'",
                true,
            ),
            (
                vec![0; FRAME_BYTES],
                b"n: 0 pts: 0 pts_time:\xff ".as_slice(),
                1.0,
                "ValueError",
                "could not convert string to float: b'\\xff'",
                true,
            ),
            (
                Vec::new(),
                b"".as_slice(),
                1.0,
                "UndecodableVideo",
                "sample.webm: native frames and timestamps are incomplete",
                true,
            ),
            (
                vec![0; FRAME_BYTES - 1],
                b"n: 0 pts: 0 pts_time:0 ".as_slice(),
                1.0,
                "UndecodableVideo",
                "sample.webm: native frames and timestamps are incomplete",
                true,
            ),
            (
                vec![0; FRAME_BYTES * (MAX_FRAMES + 1)],
                b"n: 0 pts: 0 pts_time:0 ".as_slice(),
                1.0,
                "UndecodableVideo",
                "sample.webm: native frames and timestamps are incomplete",
                true,
            ),
            (
                vec![0; FRAME_BYTES * 2],
                b"n: 0 pts: 0 pts_time:0 n: 1 pts: 0 pts_time:0 ".as_slice(),
                1.0,
                "UndecodableVideo",
                "sample.webm: native frame timestamps cannot be aligned",
                true,
            ),
            (
                vec![0; FRAME_BYTES],
                b"n: 0 pts: 0 pts_time:NaN ".as_slice(),
                1.0,
                "UndecodableVideo",
                "sample.webm: native frame timestamps cannot be aligned",
                true,
            ),
            (
                vec![0; FRAME_BYTES],
                b"n: 0 pts: 0 pts_time:0 ".as_slice(),
                0.0,
                "UndecodableVideo",
                "sample.webm: complete video duration is outside the comparison limit",
                true,
            ),
            (
                vec![0; FRAME_BYTES],
                b"n: 0 pts: 0 pts_time:2 ".as_slice(),
                1.0,
                "UndecodableVideo",
                "sample.webm: final frame duration cannot be established",
                true,
            ),
        ] {
            let error = parse_typed(source, raw, stderr, duration).err().unwrap();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, message);
            assert_eq!(error.handled, handled);
        }
        assert_eq!(
            parse(vec![0; FRAME_BYTES], b"n: 0 pts: 0 pts_time:bad ", 1.0)
                .err()
                .unwrap(),
            "invalid native frame timestamp"
        );
    }

    #[test]
    fn timeline_still_requires_finite_positive_bounded_lossless_probe_duration() {
        for duration in ["NaN", "Infinity", "-Infinity", "0", "3.1"] {
            let raw = format!(r#"{{"format":{{"duration":{duration}}}}}"#);
            let info = crate::codec_video::VideoInfo::from_stdout(raw.as_bytes()).unwrap();
            assert_eq!(
                parse(
                    vec![0; FRAME_BYTES],
                    b"n: 0 pts: 0 pts_time:0 ",
                    info.duration
                )
                .err()
                .unwrap(),
                "native frame timestamps cannot be aligned"
            );
        }
        let info =
            crate::codec_video::VideoInfo::from_stdout(br#"{"format":{"duration":1}}"#).unwrap();
        assert_eq!(
            parse(
                vec![0; FRAME_BYTES],
                b"n: 0 pts: 0 pts_time:0 ",
                info.duration
            )
            .unwrap()
            .duration,
            1.0
        );
    }
}
