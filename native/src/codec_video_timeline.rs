//! Native presentation-time comparison; the frozen lookup digest never proves video equality.
use crate::codec_video::VideoDecoder;
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

pub fn parse(raw: Vec<u8>, stderr: &[u8], duration: f64) -> Result<Timeline, String> {
    let text = std::str::from_utf8(stderr).map_err(|_| "invalid native video timing text")?;
    let mut times = Vec::new();
    for line in text.lines() {
        // Frame records have all three fields; showinfo configuration lines do not.
        if line.contains(" n:")
            && line.contains(" pts:")
            && let Some((_, suffix)) = line.split_once("pts_time:")
        {
            let token = suffix
                .split_whitespace()
                .next()
                .ok_or("missing native frame timestamp")?;
            times.push(
                token
                    .parse::<f64>()
                    .map_err(|_| "invalid native frame timestamp")?,
            );
        }
    }
    let count = raw.len() / FRAME_BYTES;
    if !raw.len().is_multiple_of(FRAME_BYTES)
        || count == 0
        || count > MAX_FRAMES
        || times.len() != count
    {
        return Err("native frames and timestamps are incomplete".into());
    }
    if !duration.is_finite()
        || duration <= 0.0
        || duration > 3.05
        || times.iter().any(|value| !value.is_finite())
        || times.windows(2).any(|pair| pair[1] <= pair[0])
    {
        return Err("native frame timestamps cannot be aligned".into());
    }
    let origin = times[0];
    let duration = duration - origin;
    for time in &mut times {
        *time -= origin;
    }
    let tail = duration - times[count - 1];
    if !(tail > 0.0 && tail <= 3.0) {
        return Err("final frame duration cannot be established".into());
    }
    Ok(Timeline {
        rgba: Arc::new(raw),
        times,
        duration,
    })
}

pub(crate) async fn decode(decoder: &VideoDecoder, path: &Path) -> Result<Timeline, String> {
    let info = decoder.probe(path).await?;
    let duration = info["duration"]
        .as_f64()
        .ok_or("complete video duration is unknown")?;
    if !duration.is_finite() || duration <= 0.0 || duration > 3.05 {
        return Err("complete video duration is outside the comparison limit".into());
    }
    let mut args = vec!["-nostdin".into(), "-v".into(), "info".into()];
    args.extend(decoder.decoder_args(path).await?);
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
    let result = crate::codec_process::capture(
        &decoder.root,
        &decoder.ffmpeg,
        &args,
        ((MAX_FRAMES + 1) * FRAME_BYTES) as u64,
    )
    .await?;
    parse(result.stdout, &result.stderr, duration)
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
            return None;
        }
    }
    if (left.duration - right.duration).abs() > 1.0 / 30.0 + 0.002 {
        return Some(false);
    }
    for (own, other) in [(left, right), (right, left)] {
        for (index, &start) in own.times.iter().enumerate() {
            let end = own.times.get(index + 1).copied().unwrap_or(own.duration);
            let matched = at(&other.times, (start + end) / 2.0);
            if !crate::codec_image::same_frame(frame(own, index)?, frame(other, matched)?)? {
                return Some(false);
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
        if !crate::codec_image::same_frame(
            frame(left, at(&left.times, midpoint))?,
            frame(right, at(&right.times, midpoint))?,
        )? {
            let rounded = end - start <= 0.002 + 1e-9
                && ((left.times[1..].contains(&start) && right.times[1..].contains(&end))
                    || (right.times[1..].contains(&start) && left.times[1..].contains(&end)));
            if !rounded {
                return Some(false);
            }
        }
    }
    Some(true)
}
