//! Closed argv recipes from the admitted original media conversion and validation assertions.
pub(super) enum Recipe {
    Conversion { input: usize, output: usize },
    Lavfi { output: usize },
    AlphaFrame { input: usize },
    Preview { input: usize },
    PreviewEncode { input: usize, output: usize },
}

pub(super) fn classify(args: &[&str], ffmpeg: &str) -> Option<Recipe> {
    if args.first().copied() != Some(ffmpeg) {
        return None;
    }
    if args.len() == 17
        && args[..6] == [ffmpeg, "-v", "error", "-c:v", "libvpx-vp9", "-i"]
        && matches!(args[8], "1" | "90")
        && args[7] == "-frames:v"
        && args[9..13] == ["-an", "-threads", "1", "-vf"]
        && match args[8] {
            "1" => matches!(
                args[13],
                "scale=104:104,format=rgba" | "scale=72:72,format=rgba"
            ),
            _ => matches!(
                args[13],
                "fps=15,scale=104:104,format=rgba" | "fps=10,scale=72:72,format=rgba"
            ),
        }
        && args[14..] == ["-f", "rawvideo", "-"]
    {
        return Some(Recipe::Preview { input: 6 });
    }
    if args.len() == 27
        && args[..13]
            == [
                ffmpeg,
                "-y",
                "-v",
                "error",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "100x100",
                "-framerate",
                "30",
                "-i",
            ]
        && args[14..26]
            == [
                "-c:v",
                "libvpx-vp9",
                "-threads",
                "1",
                "-cpu-used",
                "8",
                "-lossless",
                "1",
                "-pix_fmt",
                "yuva420p",
                "-auto-alt-ref",
                "0",
            ]
    {
        return Some(Recipe::PreviewEncode {
            input: 13,
            output: 26,
        });
    }
    if args.len() == 16
        && args[..6] == [ffmpeg, "-v", "error", "-c:v", "libvpx-vp9", "-i"]
        && args[7..]
            == [
                "-frames:v",
                "1",
                "-vf",
                "format=rgba",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-",
            ]
    {
        return Some(Recipe::AlphaFrame { input: 6 });
    }
    if matches!(args.len(), 24 | 26) && args[..4] == [ffmpeg, "-y", "-t", "3.0"] {
        let offset = if args.len() == 26 {
            if args[4..6] != ["-c:v", "libvpx-vp9"] {
                return None;
            }
            2
        } else {
            0
        };
        let input = 5 + offset;
        let tail = 6 + offset;
        if args[4 + offset] != "-i"
            || args[tail..tail + 10]
                != [
                    "-an",
                    "-vf",
                    "fps=30,scale=w=100:h=100:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba,pad=100:100:(ow-iw)/2:(oh-ih)/2:color=0x00000000,format=yuva420p",
                    "-c:v",
                    "libvpx-vp9",
                    "-pix_fmt",
                    "yuva420p",
                    "-b:v",
                    "0",
                    "-crf",
                ]
            || !matches!(args[tail + 10], "32" | "40" | "48" | "56" | "63")
            || args[tail + 11..args.len() - 1]
                != ["-auto-alt-ref", "0", "-deadline", "good", "-cpu-used", "2"]
        {
            return None;
        }
        return Some(Recipe::Conversion {
            input,
            output: args.len() - 1,
        });
    }
    if args.len() < 6 || args[..5] != [ffmpeg, "-y", "-f", "lavfi", "-i"] {
        return None;
    }
    let normal = "testsrc2=size=100x100:rate=30:duration=2";
    let vp9 = [
        "-c:v",
        "libvpx-vp9",
        "-pix_fmt",
        "yuva420p",
        "-crf",
        "50",
        "-b:v",
        "0",
    ];
    let accepted = match args.len() {
        16 => {
            matches!(
                args[5],
                "testsrc2=size=100x100:rate=30:duration=2"
                    | "testsrc2=size=100x100:rate=60:duration=2"
                    | "testsrc2=size=512x512:rate=30:duration=2"
            ) && args[6..14] == vp9
                && args[14] == "-an"
        }
        14 => {
            args[5] == normal && args[6..13] == ["-c:v", "libvpx", "-crf", "50", "-b:v", "0", "-an"]
        }
        18 => args[5] == normal && args[6..14] == vp9 && args[14..17] == ["-an", "-f", "mp4"],
        22 => {
            args[5] == normal
                && args[6..10] == ["-f", "lavfi", "-i", "sine=frequency=440:duration=2"]
                && args[10..18] == vp9
                && args[18..21] == ["-c:a", "libopus", "-shortest"]
        }
        _ => false,
    };
    accepted.then_some(Recipe::Lavfi {
        output: args.len() - 1,
    })
}
