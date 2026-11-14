//! First-party raster operations. No interpreter, subprocess, or Python ABI is used here.
//!
//! Identity uses Pillow's 22-bit separable convolution and 8-bit alpha arithmetic. In
//! particular, replacing this with `image::imageops::resize` changes catalogue keys.
//! PNG and WebP *encoded bytes* may differ from historical Pillow releases; lossless
//! decoded pixels and the existing normalized content identity remain the contract.
use image::{
    AnimationDecoder, DynamicImage, ImageDecoder, ImageEncoder, Rgb, RgbImage, Rgba, RgbaImage,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{Cursor, Read};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterError(pub String);
impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for RasterError {}
impl From<image::ImageError> for RasterError {
    fn from(error: image::ImageError) -> Self {
        Self(error.to_string())
    }
}
impl From<std::io::Error> for RasterError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
type Result<T> = std::result::Result<T, RasterError>;

/// Bounds apply before allocating decoded pixel buffers, including animation canvases.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub max_dimension: u32,
    pub max_pixels: u64,
    pub max_encoded_bytes: u64,
    pub max_decoded_bytes: u64,
    pub max_frames: usize,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_dimension: 16_384,
            max_pixels: 64 * 1024 * 1024,
            max_encoded_bytes: 128 * 1024 * 1024,
            max_decoded_bytes: 512 * 1024 * 1024,
            max_frames: 512,
        }
    }
}
impl DecodeLimits {
    fn encoded(self, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u64 > self.max_encoded_bytes {
            return Err(RasterError("encoded image exceeds byte limit".into()));
        }
        Ok(())
    }
    fn dimensions(self, width: u32, height: u32) -> Result<usize> {
        let pixels = u64::from(width) * u64::from(height);
        let bytes = pixels.checked_mul(4);
        if width == 0
            || height == 0
            || width > self.max_dimension
            || height > self.max_dimension
            || pixels > self.max_pixels
            || bytes.is_none_or(|bytes| bytes > self.max_decoded_bytes)
        {
            return Err(RasterError("image dimensions exceed decode limits".into()));
        }
        usize::try_from(bytes.expect("checked above"))
            .map_err(|_| RasterError("image allocation overflow".into()))
    }
    fn image_limits(self) -> image::Limits {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(self.max_dimension);
        limits.max_image_height = Some(self.max_dimension);
        limits.max_alloc = Some(self.max_decoded_bytes);
        limits
    }
}

#[derive(Clone, Debug)]
pub struct DecodedRaster {
    pub rgba: RgbaImage,
    /// Historical source mode: dHash premultiplies only an original RGBA raster.
    pub mode: String,
    pub animated: bool,
}

#[derive(Clone, Debug)]
pub struct RasterFrame {
    pub rgba: RgbaImage,
    pub duration_ms: u32,
}
#[derive(Clone, Debug)]
pub struct RasterAnimation {
    pub frames: Vec<RasterFrame>,
    /// 0 means repeat indefinitely, as in WebP's ANIM chunk.
    pub loop_count: u16,
}

pub fn open(path: &Path) -> Result<DecodedRaster> {
    decode(&read_bounded(path, DecodeLimits::default())?)
}
pub fn read_bounded(path: &Path, limits: DecodeLimits) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > limits.max_encoded_bytes {
        return Err(RasterError("encoded image exceeds byte limit".into()));
    }
    let mut bytes = Vec::new();
    file.take(limits.max_encoded_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    limits.encoded(&bytes)?;
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<DecodedRaster> {
    decode_with_limits(bytes, DecodeLimits::default())
}
pub fn decode_with_limits(bytes: &[u8], limits: DecodeLimits) -> Result<DecodedRaster> {
    limits.encoded(bytes)?;
    let format = image::guess_format(bytes)?;
    if format == image::ImageFormat::WebP {
        return webp::decode_first(bytes, limits);
    }
    if format == image::ImageFormat::Jpeg {
        return decode_jpeg(bytes, limits);
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits.image_limits());
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    limits.dimensions(width, height)?;
    let color = decoder.original_color_type();
    let mode = source_mode(bytes, format, color);
    let dynamic = DynamicImage::from_decoder(decoder)?;
    // Pillow's I;16 -> RGBA conversion clamps integer luminance, rather than
    // scaling the sixteen-bit range down to eight bits.
    let rgba = match dynamic {
        DynamicImage::ImageLuma16(gray) => RgbaImage::from_fn(width, height, |x, y| {
            let luminance = gray[(x, y)][0].min(255) as u8;
            Rgba([luminance, luminance, luminance, 255])
        }),
        DynamicImage::ImageLumaA16(gray) if mode == "I;16" => {
            // Pillow applies PNG integer luminance transparency after clamping
            // to L and casts the transparency value to an eight-bit sample.
            let transparent = png_chunk(bytes, b"tRNS")
                .and_then(|chunk| chunk.get(..2))
                .map(|value| u16::from_be_bytes([value[0], value[1]]) as u8);
            RgbaImage::from_fn(width, height, |x, y| {
                let luminance = gray[(x, y)][0].min(255) as u8;
                Rgba([
                    luminance,
                    luminance,
                    luminance,
                    if transparent == Some(luminance) {
                        0
                    } else {
                        255
                    },
                ])
            })
        }
        DynamicImage::ImageLumaA16(gray) => RgbaImage::from_fn(width, height, |x, y| {
            let pixel = gray[(x, y)];
            let luminance = (pixel[0] >> 8) as u8;
            Rgba([luminance, luminance, luminance, (pixel[1] >> 8) as u8])
        }),
        DynamicImage::ImageRgb16(rgb) => RgbaImage::from_fn(width, height, |x, y| {
            let pixel = rgb[(x, y)];
            Rgba([
                (pixel[0] >> 8) as u8,
                (pixel[1] >> 8) as u8,
                (pixel[2] >> 8) as u8,
                255,
            ])
        }),
        DynamicImage::ImageRgba16(rgba) => RgbaImage::from_fn(width, height, |x, y| {
            let pixel = rgba[(x, y)];
            Rgba(pixel.0.map(|value| (value >> 8) as u8))
        }),
        image => image.into_rgba8(),
    };
    let animated = match format {
        image::ImageFormat::Gif => gif_has_multiple_frames(bytes, limits)?,
        image::ImageFormat::Png => png_is_animated(bytes),
        _ => false,
    };
    Ok(DecodedRaster {
        rgba,
        mode,
        animated,
    })
}

fn decode_jpeg(bytes: &[u8], limits: DecodeLimits) -> Result<DecodedRaster> {
    // JPEG's integer IDCT and chroma upsampling form part of the old pixel
    // identity. zune-jpeg's valid but different rounding changes catalogue keys;
    // the bundled libjpeg implementation retains Pillow's decoder arithmetic.
    // mozjpeg reports fatal codec errors by unwinding; contain that boundary and
    // turn malformed input into an ordinary raster error.
    std::panic::catch_unwind(|| -> Result<DecodedRaster> {
        let mut decoder = mozjpeg::Decompress::new_mem(bytes)?;
        let (width, height) = decoder.size();
        let width = u32::try_from(width).map_err(|_| RasterError("JPEG width overflow".into()))?;
        let height =
            u32::try_from(height).map_err(|_| RasterError("JPEG height overflow".into()))?;
        limits.dimensions(width, height)?;
        decoder.dct_method(mozjpeg::DctMethod::IntegerSlow);
        decoder.do_fancy_upsampling(true);
        let source = decoder.color_space();
        let cmyk = matches!(
            source,
            mozjpeg::ColorSpace::JCS_CMYK | mozjpeg::ColorSpace::JCS_YCCK
        );
        let mode = if cmyk {
            "CMYK"
        } else if source == mozjpeg::ColorSpace::JCS_GRAYSCALE {
            "L"
        } else {
            "RGB"
        };
        let mut started = if cmyk {
            decoder.to_colorspace(mozjpeg::ColorSpace::JCS_CMYK)?
        } else {
            decoder.rgba()?
        };
        let mut raw: Vec<u8> = started.read_scanlines()?;
        started.finish()?;
        if cmyk {
            for pixel in raw.as_chunks_mut::<4>().0 {
                for channel in 0..3 {
                    pixel[channel] = multiply_rounded(pixel[channel], pixel[3]);
                }
                pixel[3] = 255;
            }
        }
        let rgba = RgbaImage::from_raw(width, height, raw)
            .ok_or_else(|| RasterError("incomplete JPEG pixels".into()))?;
        Ok(DecodedRaster {
            rgba,
            mode: mode.into(),
            animated: false,
        })
    })
    .unwrap_or_else(|error| {
        let message = error
            .downcast_ref::<String>()
            .map(String::as_str)
            .unwrap_or("JPEG decoder failed");
        Err(RasterError(message.into()))
    })
}

fn source_mode(
    bytes: &[u8],
    format: image::ImageFormat,
    color: image::ExtendedColorType,
) -> String {
    if format == image::ImageFormat::Gif {
        return "P".into();
    }
    if format == image::ImageFormat::Png && bytes.len() >= 26 {
        return match bytes[25] {
            0 if bytes[24] == 1 => "1",
            0 if bytes[24] == 16 => "I;16",
            0 => "L",
            2 => "RGB",
            3 => "P",
            4 if bytes[24] == 16 => "RGBA",
            4 => "LA",
            6 => "RGBA",
            _ => "RGBA",
        }
        .into();
    }
    match color {
        image::ExtendedColorType::L1 => "1",
        image::ExtendedColorType::L8 => "L",
        image::ExtendedColorType::La8 | image::ExtendedColorType::La16 => "LA",
        image::ExtendedColorType::L16 => "I;16",
        image::ExtendedColorType::Rgb8 | image::ExtendedColorType::Rgb16 => "RGB",
        image::ExtendedColorType::Cmyk8 => "CMYK",
        _ => "RGBA",
    }
    .into()
}

fn png_chunk<'a>(bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    let mut offset = 8usize;
    while let Some(header) = bytes.get(offset..offset.saturating_add(8)) {
        let length = u32::from_be_bytes(header[..4].try_into().expect("four bytes")) as usize;
        if &header[4..8] == kind {
            return bytes.get(offset.checked_add(8)?..offset.checked_add(8)?.checked_add(length)?);
        }
        let Some(next) = offset.checked_add(length).and_then(|v| v.checked_add(12)) else {
            break;
        };
        offset = next;
    }
    None
}
fn png_is_animated(bytes: &[u8]) -> bool {
    png_chunk(bytes, b"acTL")
        .and_then(|control| control.get(..4))
        .is_some_and(|count| u32::from_be_bytes(count.try_into().expect("four bytes")) > 1)
}

fn gif_has_multiple_frames(bytes: &[u8], limits: DecodeLimits) -> Result<bool> {
    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(limits.image_limits())?;
    let mut frames = decoder.into_frames();
    if let Some(frame) = frames.next() {
        frame?;
    }
    match frames.next() {
        Some(frame) => {
            frame?;
            Ok(true)
        }
        None => Ok(false),
    }
}

pub fn decode_animation(bytes: &[u8], limits: DecodeLimits) -> Result<RasterAnimation> {
    limits.encoded(bytes)?;
    match image::guess_format(bytes)? {
        image::ImageFormat::WebP => webp::decode_animation(bytes, limits),
        image::ImageFormat::Gif => {
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
            decoder.set_limits(limits.image_limits())?;
            let (width, height) = decoder.dimensions();
            let frame_bytes = limits.dimensions(width, height)? as u64;
            let mut frames = Vec::new();
            for frame in decoder.into_frames() {
                if frames.len() >= limits.max_frames
                    || (frames.len() as u64 + 1) * frame_bytes > limits.max_decoded_bytes
                {
                    return Err(RasterError(
                        "animation exceeds frame allocation limits".into(),
                    ));
                }
                let frame = frame?;
                let (numerator, denominator) = frame.delay().numer_denom_ms();
                frames.push(RasterFrame {
                    duration_ms: ((u64::from(numerator) + u64::from(denominator) / 2)
                        / u64::from(denominator))
                    .min(u64::from(u32::MAX)) as u32,
                    rgba: frame.into_buffer(),
                });
            }
            Ok(RasterAnimation {
                frames,
                loop_count: gif_loop_count(bytes).unwrap_or(1),
            })
        }
        image::ImageFormat::Png if png_chunk(bytes, b"acTL").is_some() => {
            let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))?;
            decoder.set_limits(limits.image_limits())?;
            let (width, height) = decoder.dimensions();
            let frame_bytes = limits.dimensions(width, height)? as u64;
            let mut frames = Vec::new();
            for frame in decoder.apng()?.into_frames() {
                if frames.len() >= limits.max_frames
                    || (frames.len() as u64 + 1) * frame_bytes > limits.max_decoded_bytes
                {
                    return Err(RasterError(
                        "animation exceeds frame allocation limits".into(),
                    ));
                }
                let frame = frame?;
                let (numerator, denominator) = frame.delay().numer_denom_ms();
                frames.push(RasterFrame {
                    rgba: frame.into_buffer(),
                    duration_ms: ((u64::from(numerator) + u64::from(denominator) / 2)
                        / u64::from(denominator))
                    .min(u64::from(u32::MAX)) as u32,
                });
            }
            let loop_count = png_chunk(bytes, b"acTL")
                .and_then(|chunk| chunk.get(4..8))
                .map(|count| {
                    u32::from_be_bytes(count.try_into().expect("four bytes"))
                        .min(u32::from(u16::MAX)) as u16
                })
                .unwrap_or(0);
            Ok(RasterAnimation { frames, loop_count })
        }
        _ => {
            let decoded = decode_with_limits(bytes, limits)?;
            Ok(RasterAnimation {
                frames: vec![RasterFrame {
                    rgba: decoded.rgba,
                    duration_ms: 0,
                }],
                loop_count: 1,
            })
        }
    }
}

fn gif_loop_count(bytes: &[u8]) -> Option<u16> {
    // Application extension has a fixed signature followed by sub-block 03 01 lo hi.
    for (index, chunk) in bytes.windows(14).enumerate() {
        if chunk.starts_with(b"\x21\xff\x0bNETSCAPE2.0")
            || chunk.starts_with(b"\x21\xff\x0bANIMEXTS1.0")
        {
            let block = bytes.get(index + 14..index + 18)?;
            if block[..2] == [3, 1] {
                return Some(u16::from_le_bytes([block[2], block[3]]));
            }
        }
    }
    None
}

pub fn encode_png(rgba: &RgbaImage) -> Result<Vec<u8>> {
    DecodeLimits::default().dimensions(rgba.width(), rgba.height())?;
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut bytes,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(bytes)
}

#[derive(Clone, Copy, Debug)]
pub struct WebpOptions {
    pub lossless: bool,
    pub quality: f32,
    pub method: i32,
}
impl Default for WebpOptions {
    fn default() -> Self {
        Self {
            lossless: true,
            quality: 80.0,
            method: 4,
        }
    }
}
pub fn encode_webp(rgba: &RgbaImage, options: WebpOptions) -> Result<Vec<u8>> {
    webp::encode(rgba, options)
}
pub fn encode_animated_webp(animation: &RasterAnimation, options: WebpOptions) -> Result<Vec<u8>> {
    webp::encode_animation(animation, options)
}

/// Uses Pillow's animation optimization defaults. Duplicate frames may merge,
/// spatial deltas may replace full frames, and invisible RGB may be discarded.
pub fn encode_pillow_animated_webp(
    animation: &RasterAnimation,
    options: WebpOptions,
) -> Result<Vec<u8>> {
    webp::encode_pillow_animation(animation, options)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResampleFilter {
    Lanczos,
    Bicubic,
}

// Coefficients are normalized in f64 and rounded away from zero to 22-bit signed
// integers. Each horizontal/vertical pass clips and rounds back to eight bits.
fn coefficients(input: u32, output: u32, filter: ResampleFilter) -> Vec<(usize, Vec<i32>)> {
    let scale = f64::from(input) / f64::from(output);
    let filter_scale = scale.max(1.0);
    let support = filter_scale
        * match filter {
            ResampleFilter::Lanczos => 3.0,
            ResampleFilter::Bicubic => 2.0,
        };
    (0..output)
        .map(|position| {
            let center = (f64::from(position) + 0.5) * scale;
            let start = ((center - support + 0.5) as i64).max(0) as usize;
            let end = ((center + support + 0.5) as i64).min(i64::from(input)) as usize;
            let mut weights: Vec<f64> = (start..end)
                .map(|x| {
                    let distance = (x as f64 - center + 0.5) / filter_scale;
                    match filter {
                        ResampleFilter::Lanczos => {
                            fn sinc(value: f64) -> f64 {
                                if value == 0.0 {
                                    1.0
                                } else {
                                    let radians = value * std::f64::consts::PI;
                                    radians.sin() / radians
                                }
                            }
                            if (-3.0..3.0).contains(&distance) {
                                sinc(distance) * sinc(distance / 3.0)
                            } else {
                                0.0
                            }
                        }
                        ResampleFilter::Bicubic => {
                            let x = distance.abs();
                            if x < 1.0 {
                                (1.5 * x - 2.5) * x * x + 1.0
                            } else if x < 2.0 {
                                (((x - 5.0) * x + 8.0) * x - 4.0) * -0.5
                            } else {
                                0.0
                            }
                        }
                    }
                })
                .collect();
            let total: f64 = weights.iter().sum();
            if total != 0.0 {
                for weight in &mut weights {
                    *weight /= total;
                }
            }
            (
                start,
                weights
                    .into_iter()
                    .map(|w| (w * ((1u32 << 22) as f64)).round() as i32)
                    .collect(),
            )
        })
        .collect()
}

fn resample(
    raw: &[u8],
    width: u32,
    height: u32,
    channels: usize,
    target_width: u32,
    target_height: u32,
    filter: ResampleFilter,
) -> Result<Vec<u8>> {
    DecodeLimits::default().dimensions(target_width, target_height)?;
    if width == 0 || height == 0 {
        return Err(RasterError("cannot resize empty image".into()));
    }
    // Pillow resizes exceptionally tall rasters vertically first. The pass
    // order matters because both passes round to eight bits independently.
    if u64::from(height) > u64::from(width) * 100 && target_height < height {
        let transpose = |pixels: &[u8], width: u32, height: u32| {
            let mut transposed = vec![0; pixels.len()];
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let src = (y * width as usize + x) * channels;
                    let dst = (x * height as usize + y) * channels;
                    transposed[dst..dst + channels].copy_from_slice(&pixels[src..src + channels]);
                }
            }
            transposed
        };
        let transposed = transpose(raw, width, height);
        let resized = resample(
            &transposed,
            height,
            width,
            channels,
            target_height,
            target_width,
            filter,
        )?;
        return Ok(transpose(&resized, target_height, target_width));
    }
    DecodeLimits::default().dimensions(target_width, height)?;
    let mut raster = raw.to_vec();
    let mut current_width = width as usize;
    if width != target_width {
        let weights = coefficients(width, target_width, filter);
        let mut horizontal = vec![0; target_width as usize * height as usize * channels];
        for y in 0..height as usize {
            for (x, (start, coefficients)) in weights.iter().enumerate() {
                for channel in 0..channels {
                    let mut sum = 1i64 << 21;
                    for (offset, coefficient) in coefficients.iter().enumerate() {
                        sum += i64::from(
                            raster[(y * current_width + start + offset) * channels + channel],
                        ) * i64::from(*coefficient);
                    }
                    horizontal[(y * target_width as usize + x) * channels + channel] =
                        (sum >> 22).clamp(0, 255) as u8;
                }
            }
        }
        raster = horizontal;
        current_width = target_width as usize;
    }
    if height != target_height {
        let weights = coefficients(height, target_height, filter);
        let mut vertical = vec![0; current_width * target_height as usize * channels];
        for (y, (start, coefficients)) in weights.iter().enumerate() {
            for x in 0..current_width {
                for channel in 0..channels {
                    let mut sum = 1i64 << 21;
                    for (offset, coefficient) in coefficients.iter().enumerate() {
                        sum += i64::from(
                            raster[((start + offset) * current_width + x) * channels + channel],
                        ) * i64::from(*coefficient);
                    }
                    vertical[(y * current_width + x) * channels + channel] =
                        (sum >> 22).clamp(0, 255) as u8;
                }
            }
        }
        raster = vertical;
    }
    Ok(raster)
}

fn multiply_rounded(value: u8, alpha: u8) -> u8 {
    let value = u32::from(value) * u32::from(alpha) + 128;
    (((value >> 8) + value) >> 8) as u8
}

pub fn resize_rgba(
    rgba: &RgbaImage,
    width: u32,
    height: u32,
    filter: ResampleFilter,
) -> Result<RgbaImage> {
    DecodeLimits::default().dimensions(width, height)?;
    if rgba.dimensions() == (width, height) {
        return Ok(rgba.clone());
    }
    let mut premultiplied = rgba.as_raw().clone();
    for pixel in premultiplied.as_chunks_mut::<4>().0 {
        for channel in 0..3 {
            pixel[channel] = multiply_rounded(pixel[channel], pixel[3]);
        }
    }
    let mut raw = resample(
        &premultiplied,
        rgba.width(),
        rgba.height(),
        4,
        width,
        height,
        filter,
    )?;
    for pixel in raw.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha != 0 && alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = (u32::from(*channel) * 255 / alpha).min(255) as u8;
            }
        }
    }
    RgbaImage::from_raw(width, height, raw)
        .ok_or_else(|| RasterError("invalid resized raster".into()))
}
pub fn resize_lanczos(rgba: &RgbaImage, width: u32, height: u32) -> Result<RgbaImage> {
    resize_rgba(rgba, width, height, ResampleFilter::Lanczos)
}
pub fn normalized_rgba(rgba: &RgbaImage) -> Result<RgbaImage> {
    resize_lanczos(rgba, 64, 64)
}

pub fn alpha_bounds(rgba: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut left, mut top, mut right, mut bottom) = (rgba.width(), rgba.height(), 0, 0);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] != 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    (left < right && top < bottom).then_some((left, top, right, bottom))
}
pub fn fit_canvas(rgba: &RgbaImage, side: u32) -> Result<RgbaImage> {
    DecodeLimits::default().dimensions(side, side)?;
    let mut canvas = RgbaImage::new(side, side);
    if rgba.width() == 0 || rgba.height() == 0 {
        return Ok(canvas);
    }
    let trimmed = if let Some((left, top, right, bottom)) = alpha_bounds(rgba) {
        image::imageops::crop_imm(rgba, left, top, right - left, bottom - top).to_image()
    } else {
        rgba.clone()
    };
    let scale = (f64::from(side) / f64::from(trimmed.width()))
        .min(f64::from(side) / f64::from(trimmed.height()));
    let width = (f64::from(trimmed.width()) * scale)
        .round_ties_even()
        .max(1.0) as u32;
    let height = (f64::from(trimmed.height()) * scale)
        .round_ties_even()
        .max(1.0) as u32;
    let resized = resize_lanczos(&trimmed, width, height)?;
    // Unmasked paste: a second alpha composition would darken partial-alpha pixels.
    image::imageops::replace(
        &mut canvas,
        &resized,
        i64::from((side - width) / 2),
        i64::from((side - height) / 2),
    );
    Ok(canvas)
}
pub fn thumbnail(rgba: &RgbaImage, side: u32, filter: ResampleFilter) -> Result<RgbaImage> {
    if side == 0 {
        return Err(RasterError("thumbnail size must be positive".into()));
    }
    if rgba.width() <= side && rgba.height() <= side {
        return Ok(rgba.clone());
    }
    if rgba.width() == 0 || rgba.height() == 0 {
        return Err(RasterError("cannot thumbnail empty image".into()));
    }
    let aspect = f64::from(rgba.width()) / f64::from(rgba.height());
    let rounded = |value: f64, aspect_error: &dyn Fn(u32) -> f64| {
        let floor = value.floor().max(1.0) as u32;
        let ceil = value.ceil().max(1.0) as u32;
        if aspect_error(floor) <= aspect_error(ceil) {
            floor
        } else {
            ceil
        }
    };
    let (width, height) = if rgba.width() > rgba.height() {
        (
            side,
            rounded(f64::from(side) / aspect, &|h| {
                (aspect - f64::from(side) / f64::from(h)).abs()
            }),
        )
    } else {
        (
            rounded(f64::from(side) * aspect, &|w| {
                (aspect - f64::from(w) / f64::from(side)).abs()
            }),
            side,
        )
    };
    resize_rgba(rgba, width, height, filter)
}

pub fn blank_rgba(raw: &[u8]) -> bool {
    raw.iter()
        .skip(3)
        .step_by(4)
        .filter(|&&alpha| alpha > 10)
        .count()
        <= 8
}
pub fn is_blank(rgba: &RgbaImage) -> bool {
    blank_rgba(rgba.as_raw())
}
pub fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn content_key(rgba: &RgbaImage) -> Result<String> {
    Ok(format!(
        "s:{}",
        &digest_hex(normalized_rgba(rgba)?.as_raw())[..32]
    ))
}
pub fn audit_signature(rgba: &RgbaImage) -> Result<String> {
    Ok(digest_hex(normalized_rgba(rgba)?.as_raw())[..24].into())
}

/// ImageChops.multiply uses floor, unlike RGBA -> RGBa's rounded multiplication.
pub fn premultiplied_pixels(rgba: &RgbaImage) -> Vec<u8> {
    let mut raw = rgba.as_raw().clone();
    for pixel in raw.as_chunks_mut::<4>().0 {
        for channel in 0..3 {
            pixel[channel] = (u16::from(pixel[channel]) * u16::from(pixel[3]) / 255) as u8;
        }
    }
    raw
}
pub fn dhash_mode(rgba: &RgbaImage, premultiply: bool) -> Result<u64> {
    let raw = if premultiply {
        premultiplied_pixels(rgba)
    } else {
        rgba.as_raw().clone()
    };
    let gray: Vec<u8> = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            ((u32::from(pixel[0]) * 19595
                + u32::from(pixel[1]) * 38470
                + u32::from(pixel[2]) * 7471
                + 32768)
                >> 16) as u8
        })
        .collect();
    let resized = resample(
        &gray,
        rgba.width(),
        rgba.height(),
        1,
        9,
        8,
        ResampleFilter::Lanczos,
    )?;
    let mut bits = 0u64;
    for row in resized.as_chunks::<9>().0 {
        for pair in row.windows(2) {
            bits = (bits << 1) | u64::from(pair[0] > pair[1]);
        }
    }
    Ok(bits)
}
pub fn dhash(rgba: &RgbaImage) -> Result<u64> {
    dhash_mode(rgba, true)
}
pub fn source_dhash(decoded: &DecodedRaster) -> Result<u64> {
    dhash_mode(&decoded.rgba, decoded.mode == "RGBA")
}
pub fn frame_phash(raw: &[u8]) -> Option<u64> {
    RgbaImage::from_raw(64, 64, raw.get(..64 * 64 * 4)?.to_vec()).and_then(|rgba| dhash(&rgba).ok())
}
pub fn close_pixels(left: &[u8], right: &[u8]) -> Option<bool> {
    if left.len() != 64 * 64 * 4 || right.len() != left.len() {
        return None;
    }
    let sum: u64 = left
        .iter()
        .zip(right)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    Some(sum <= 8 * 64 * 64 * 4)
}
pub fn same_rgba(left: &RgbaImage, right: &RgbaImage) -> Result<bool> {
    if content_key(left)? == content_key(right)? {
        return Ok(true);
    }
    if (dhash(left)? ^ dhash(right)?).count_ones() > 6 {
        return Ok(false);
    }
    Ok(close_pixels(
        &premultiplied_pixels(&normalized_rgba(left)?),
        &premultiplied_pixels(&normalized_rgba(right)?),
    )
    .expect("two normalized rasters"))
}
pub fn same_decoded(left: &DecodedRaster, right: &DecodedRaster) -> Result<bool> {
    if content_key(&left.rgba)? == content_key(&right.rgba)? {
        return Ok(true);
    }
    if (source_dhash(left)? ^ source_dhash(right)?).count_ones() > 6 {
        return Ok(false);
    }
    Ok(close_pixels(
        &premultiplied_pixels(&normalized_rgba(&left.rgba)?),
        &premultiplied_pixels(&normalized_rgba(&right.rgba)?),
    )
    .expect("two normalized rasters"))
}
pub fn same_frame(left: &[u8], right: &[u8]) -> Option<bool> {
    if left.len() != 64 * 64 * 4 || right.len() != left.len() {
        return None;
    }
    let left = RgbaImage::from_raw(64, 64, left.to_vec())?;
    let right = RgbaImage::from_raw(64, 64, right.to_vec())?;
    // Frame comparison has no exact-content shortcut in the historical contract.
    if (dhash(&left).ok()? ^ dhash(&right).ok()?).count_ones() > 6 {
        return Some(false);
    }
    close_pixels(&premultiplied_pixels(&left), &premultiplied_pixels(&right))
}

pub fn tint(rgba: &RgbaImage, rgb: [u8; 3]) -> RgbaImage {
    RgbaImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        Rgba([rgb[0], rgb[1], rgb[2], rgba[(x, y)][3]])
    })
}
pub fn on_white(rgba: &RgbaImage) -> RgbImage {
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let pixel = rgba[(x, y)];
        Rgb(std::array::from_fn(|channel| {
            let numerator =
                u32::from(pixel[channel]) * u32::from(pixel[3]) + 255 * (255 - u32::from(pixel[3]));
            let temporary = numerator + 128;
            (((temporary >> 8) + temporary) >> 8) as u8
        }))
    })
}
pub fn remap_signature(rgba: &RgbaImage) -> Result<Vec<u8>> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    let mut alpha = Vec::with_capacity(rgba.len() / 4);
    for pixel in rgba.pixels() {
        for channel in 0..3 {
            rgb.push(multiply_rounded(pixel[channel], pixel[3]));
        }
        alpha.push(pixel[3]);
    }
    let mut result = resample(
        &rgb,
        rgba.width(),
        rgba.height(),
        3,
        16,
        16,
        ResampleFilter::Lanczos,
    )?;
    result.extend(resample(
        &alpha,
        rgba.width(),
        rgba.height(),
        1,
        16,
        16,
        ResampleFilter::Lanczos,
    )?);
    Ok(result)
}
pub fn logo_distance(ours: &RgbaImage, theirs: &RgbaImage) -> Result<u32> {
    let want = dhash(theirs)?;
    let direct = (dhash(ours)? ^ want).count_ones();
    let inverse = RgbaImage::from_fn(ours.width(), ours.height(), |x, y| {
        let pixel = ours[(x, y)];
        Rgba([255 - pixel[0], 255 - pixel[1], 255 - pixel[2], 255])
    });
    let white = DynamicImage::ImageRgb8(on_white(ours)).into_rgba8();
    Ok(direct
        .min((dhash(&inverse)? ^ want).count_ones())
        .min((dhash(&white)? ^ want).count_ones()))
}

// The only unsafe boundary is the bundled native WebP codec. Each allocation is
// owned by an RAII guard; input/output buffers remain alive for the entire FFI call.
#[allow(unsafe_code)]
mod webp {
    use super::*;
    use libwebp_sys as ffi;
    use std::{mem::MaybeUninit, ptr, slice};

    struct Picture(ffi::WebPPicture);
    impl Drop for Picture {
        fn drop(&mut self) {
            unsafe { ffi::WebPPictureFree(&mut self.0) }
        }
    }
    struct Writer(ffi::WebPMemoryWriter);
    impl Drop for Writer {
        fn drop(&mut self) {
            unsafe { ffi::WebPMemoryWriterClear(&mut self.0) }
        }
    }
    struct Decoder(*mut ffi::WebPAnimDecoder);
    impl Drop for Decoder {
        fn drop(&mut self) {
            unsafe { ffi::WebPAnimDecoderDelete(self.0) }
        }
    }
    struct Mux(*mut ffi::WebPMux);
    impl Drop for Mux {
        fn drop(&mut self) {
            unsafe { ffi::WebPMuxDelete(self.0) }
        }
    }
    struct Data(ffi::WebPData);
    impl Drop for Data {
        fn drop(&mut self) {
            unsafe { ffi::WebPDataClear(&mut self.0) }
        }
    }
    struct AnimationEncoder(*mut ffi::WebPAnimEncoder);
    impl Drop for AnimationEncoder {
        fn drop(&mut self) {
            unsafe { ffi::WebPAnimEncoderDelete(self.0) }
        }
    }

    fn fail(message: &str) -> RasterError {
        RasterError(format!("WebP {message}"))
    }
    fn features(bytes: &[u8], limits: DecodeLimits) -> Result<ffi::WebPBitstreamFeatures> {
        limits.encoded(bytes)?;
        let mut features = MaybeUninit::uninit();
        // SAFETY: bytes is valid for its length; features is initialized on success.
        let status =
            unsafe { ffi::WebPGetFeatures(bytes.as_ptr(), bytes.len(), features.as_mut_ptr()) };
        if status != ffi::VP8StatusCode::VP8_STATUS_OK {
            return Err(fail("header is invalid"));
        }
        let features = unsafe { features.assume_init() };
        limits.dimensions(features.width as u32, features.height as u32)?;
        Ok(features)
    }
    pub(super) fn decode_first(bytes: &[u8], limits: DecodeLimits) -> Result<DecodedRaster> {
        let features = features(bytes, limits)?;
        let rgba = if features.has_animation != 0 {
            animation_frames(bytes, limits, true)?.frames.remove(0).rgba
        } else {
            let (width, height) = (features.width as u32, features.height as u32);
            let mut rgba = RgbaImage::new(width, height);
            // SAFETY: dimensions and stride were checked, and output is width*height*4 bytes.
            let decoded = unsafe {
                ffi::WebPDecodeRGBAInto(
                    bytes.as_ptr(),
                    bytes.len(),
                    rgba.as_mut_ptr(),
                    rgba.len(),
                    features.width * 4,
                )
            };
            if decoded.is_null() {
                return Err(fail("decode failed"));
            }
            rgba
        };
        Ok(DecodedRaster {
            rgba,
            mode: if features.has_alpha != 0 {
                "RGBA"
            } else {
                "RGB"
            }
            .into(),
            animated: features.has_animation != 0,
        })
    }
    pub(super) fn decode_animation(bytes: &[u8], limits: DecodeLimits) -> Result<RasterAnimation> {
        features(bytes, limits)?;
        animation_frames(bytes, limits, false)
    }
    fn animation_frames(
        bytes: &[u8],
        limits: DecodeLimits,
        first_only: bool,
    ) -> Result<RasterAnimation> {
        let data = ffi::WebPData {
            bytes: bytes.as_ptr(),
            size: bytes.len(),
        };
        let mut options = MaybeUninit::uninit();
        if unsafe { ffi::WebPAnimDecoderOptionsInit(options.as_mut_ptr()) } == 0 {
            return Err(fail("decoder ABI mismatch"));
        }
        let mut options = unsafe { options.assume_init() };
        options.color_mode = ffi::WEBP_CSP_MODE::MODE_RGBA;
        options.use_threads = 0;
        let decoder = Decoder(unsafe { ffi::WebPAnimDecoderNew(&data, &options) });
        if decoder.0.is_null() {
            return Err(fail("animation header is invalid"));
        }
        let mut info = ffi::WebPAnimInfo::default();
        if unsafe { ffi::WebPAnimDecoderGetInfo(decoder.0, &mut info) } == 0 {
            return Err(fail("animation info unavailable"));
        }
        let frame_bytes = limits.dimensions(info.canvas_width, info.canvas_height)?;
        if info.frame_count == 0
            || (!first_only
                && (info.frame_count as usize > limits.max_frames
                    || info.frame_count as u64 * frame_bytes as u64 > limits.max_decoded_bytes))
        {
            return Err(fail("animation exceeds frame allocation limits"));
        }
        let mut frames = Vec::new();
        let mut previous_timestamp = 0i32;
        while unsafe { ffi::WebPAnimDecoderHasMoreFrames(decoder.0) } != 0 {
            let mut pixels = ptr::null_mut();
            let mut timestamp = 0i32;
            if unsafe { ffi::WebPAnimDecoderGetNext(decoder.0, &mut pixels, &mut timestamp) } == 0
                || pixels.is_null()
                || timestamp < previous_timestamp
            {
                return Err(fail("animation frame is invalid"));
            }
            // SAFETY: libwebp guarantees a full RGBA canvas valid until GetNext/Delete.
            let raw = unsafe { slice::from_raw_parts(pixels, frame_bytes) }.to_vec();
            let rgba = RgbaImage::from_raw(info.canvas_width, info.canvas_height, raw)
                .ok_or_else(|| fail("invalid frame canvas"))?;
            frames.push(RasterFrame {
                rgba,
                duration_ms: (timestamp - previous_timestamp) as u32,
            });
            previous_timestamp = timestamp;
            if first_only {
                break;
            }
        }
        if frames.is_empty() {
            return Err(fail("animation contains no frames"));
        }
        Ok(RasterAnimation {
            frames,
            loop_count: info.loop_count.min(u32::from(u16::MAX)) as u16,
        })
    }
    pub(super) fn encode(rgba: &RgbaImage, options: WebpOptions) -> Result<Vec<u8>> {
        encode_with_exact(rgba, options, true)
    }
    fn encode_with_exact(rgba: &RgbaImage, options: WebpOptions, exact: bool) -> Result<Vec<u8>> {
        DecodeLimits::default().dimensions(rgba.width(), rgba.height())?;
        if rgba.width() > 16383 || rgba.height() > 16383 {
            return Err(fail("dimensions exceed format limits"));
        }
        if !options.quality.is_finite()
            || !(0.0..=100.0).contains(&options.quality)
            || !(0..=6).contains(&options.method)
        {
            return Err(fail("encoding options are invalid"));
        }
        let mut config = ffi::WebPConfig::new().map_err(|_| fail("encoder ABI mismatch"))?;
        config.lossless = i32::from(options.lossless);
        config.quality = options.quality;
        config.method = options.method;
        config.exact = i32::from(exact);
        if unsafe { ffi::WebPValidateConfig(&config) } == 0 {
            return Err(fail("invalid encoder config"));
        }
        let mut picture =
            Picture(ffi::WebPPicture::new().map_err(|_| fail("picture ABI mismatch"))?);
        picture.0.width = rgba.width() as i32;
        picture.0.height = rgba.height() as i32;
        picture.0.use_argb = 1;
        if unsafe {
            ffi::WebPPictureImportRGBA(&mut picture.0, rgba.as_ptr(), rgba.width() as i32 * 4)
        } == 0
        {
            return Err(fail("cannot import RGBA pixels"));
        }
        let mut writer = MaybeUninit::uninit();
        unsafe { ffi::WebPMemoryWriterInit(writer.as_mut_ptr()) };
        let mut writer = Writer(unsafe { writer.assume_init() });
        picture.0.writer = Some(ffi::WebPMemoryWrite);
        picture.0.custom_ptr = (&mut writer.0 as *mut ffi::WebPMemoryWriter).cast();
        if unsafe { ffi::WebPEncode(&config, &mut picture.0) } == 0 {
            return Err(fail(&format!(
                "encoding failed: {:?}",
                picture.0.error_code
            )));
        }
        if writer.0.mem.is_null() {
            return Err(fail("encoder returned no bytes"));
        }
        // SAFETY: the writer guard owns this allocation until the copy completes.
        Ok(unsafe { slice::from_raw_parts(writer.0.mem, writer.0.size) }.to_vec())
    }
    pub(super) fn encode_pillow_animation(
        animation: &RasterAnimation,
        options: WebpOptions,
    ) -> Result<Vec<u8>> {
        let first = animation
            .frames
            .first()
            .ok_or_else(|| fail("animation contains no frames"))?;
        let limits = DecodeLimits::default();
        let frame_bytes = limits.dimensions(first.rgba.width(), first.rgba.height())?;
        if first.rgba.width() > 16383
            || first.rgba.height() > 16383
            || animation.frames.len() > limits.max_frames
            || animation.frames.len() as u64 * frame_bytes as u64 > limits.max_decoded_bytes
        {
            return Err(fail("animation exceeds frame allocation limits"));
        }
        if animation.frames.len() == 1 {
            return encode_with_exact(&first.rgba, options, false);
        }
        if !options.quality.is_finite()
            || !(0.0..=100.0).contains(&options.quality)
            || !(0..=6).contains(&options.method)
        {
            return Err(fail("encoding options are invalid"));
        }
        let mut configuration = ffi::WebPConfig::new().map_err(|_| fail("encoder ABI mismatch"))?;
        configuration.lossless = i32::from(options.lossless);
        configuration.quality = options.quality;
        configuration.method = options.method;
        configuration.alpha_quality = 100;
        // Pillow's animated encoder leaves exact at the libwebp default, zero.
        configuration.exact = 0;
        if unsafe { ffi::WebPValidateConfig(&configuration) } == 0 {
            return Err(fail("invalid encoder config"));
        }
        let mut encoder_options = MaybeUninit::uninit();
        if unsafe {
            ffi::WebPAnimEncoderOptionsInitInternal(
                encoder_options.as_mut_ptr(),
                ffi::WEBP_MUX_ABI_VERSION as i32,
            )
        } == 0
        {
            return Err(fail("animation encoder ABI mismatch"));
        }
        let mut encoder_options = unsafe { encoder_options.assume_init() };
        encoder_options.anim_params.bgcolor = 0;
        encoder_options.anim_params.loop_count = i32::from(animation.loop_count);
        encoder_options.minimize_size = 0;
        encoder_options.kmin = if options.lossless { 9 } else { 3 };
        encoder_options.kmax = if options.lossless { 17 } else { 5 };
        encoder_options.allow_mixed = 0;
        encoder_options.verbose = 0;
        let encoder = AnimationEncoder(unsafe {
            ffi::WebPAnimEncoderNewInternal(
                first.rgba.width() as i32,
                first.rgba.height() as i32,
                &encoder_options,
                ffi::WEBP_MUX_ABI_VERSION as i32,
            )
        });
        if encoder.0.is_null() {
            return Err(fail("cannot allocate animation encoder"));
        }
        let mut timestamp = 0i32;
        for frame in &animation.frames {
            if frame.rgba.dimensions() != first.rgba.dimensions() || frame.duration_ms > 0xff_ffff {
                return Err(fail("animation frame dimensions or duration are invalid"));
            }
            let mut picture =
                Picture(ffi::WebPPicture::new().map_err(|_| fail("picture ABI mismatch"))?);
            picture.0.width = frame.rgba.width() as i32;
            picture.0.height = frame.rgba.height() as i32;
            picture.0.use_argb = 1;
            if unsafe {
                ffi::WebPPictureImportRGBA(
                    &mut picture.0,
                    frame.rgba.as_ptr(),
                    frame.rgba.width() as i32 * 4,
                )
            } == 0
            {
                return Err(fail("cannot import RGBA pixels"));
            }
            if unsafe {
                ffi::WebPAnimEncoderAdd(encoder.0, &mut picture.0, timestamp, &configuration)
            } == 0
            {
                return Err(fail("cannot encode animation frame"));
            }
            timestamp = timestamp
                .checked_add(frame.duration_ms as i32)
                .ok_or_else(|| fail("animation timeline exceeds format limits"))?;
        }
        // The terminal timestamp determines the last frame's duration; omitting
        // this call truncates clips whose final duration differs from the prior one.
        if unsafe { ffi::WebPAnimEncoderAdd(encoder.0, ptr::null_mut(), timestamp, ptr::null()) }
            == 0
        {
            return Err(fail("cannot flush animation timeline"));
        }
        let mut data = Data(ffi::WebPData::default());
        if unsafe { ffi::WebPAnimEncoderAssemble(encoder.0, &mut data.0) } == 0
            || data.0.bytes.is_null()
        {
            return Err(fail("cannot assemble animation"));
        }
        Ok(unsafe { slice::from_raw_parts(data.0.bytes, data.0.size) }.to_vec())
    }
    pub(super) fn encode_animation(
        animation: &RasterAnimation,
        options: WebpOptions,
    ) -> Result<Vec<u8>> {
        let first = animation
            .frames
            .first()
            .ok_or_else(|| fail("animation contains no frames"))?;
        let frame_bytes =
            DecodeLimits::default().dimensions(first.rgba.width(), first.rgba.height())?;
        let limits = DecodeLimits::default();
        if animation.frames.len() > limits.max_frames
            || animation.frames.len() as u64 * frame_bytes as u64 > limits.max_decoded_bytes
        {
            return Err(fail("animation exceeds frame allocation limits"));
        }
        if animation.frames.len() == 1 {
            return encode(&first.rgba, options);
        }
        let mux = Mux(ffi::WebPMuxNew());
        if mux.0.is_null() {
            return Err(fail("cannot allocate animation mux"));
        }
        let params = ffi::WebPMuxAnimParams {
            bgcolor: 0,
            loop_count: i32::from(animation.loop_count),
        };
        if unsafe { ffi::WebPMuxSetAnimationParams(mux.0, &params) }
            != ffi::WebPMuxError::WEBP_MUX_OK
        {
            return Err(fail("cannot set animation parameters"));
        }
        // Full frames with NO_BLEND deliberately avoid encoder optimizations which
        // erase hidden RGB, merge identical frames, or alter duration boundaries.
        for frame in &animation.frames {
            if frame.rgba.dimensions() != first.rgba.dimensions() || frame.duration_ms > 0xff_ffff {
                return Err(fail("animation frame dimensions or duration are invalid"));
            }
            let bytes = encode(&frame.rgba, options)?;
            let info = ffi::WebPMuxFrameInfo {
                bitstream: ffi::WebPData {
                    bytes: bytes.as_ptr(),
                    size: bytes.len(),
                },
                x_offset: 0,
                y_offset: 0,
                duration: frame.duration_ms as i32,
                id: ffi::WebPChunkId::WEBP_CHUNK_ANMF,
                dispose_method: ffi::WebPMuxAnimDispose::WEBP_MUX_DISPOSE_NONE,
                blend_method: ffi::WebPMuxAnimBlend::WEBP_MUX_NO_BLEND,
                pad: [0],
            };
            // SAFETY: copy_data=1 gives the mux its own copy before bytes is dropped.
            if unsafe { ffi::WebPMuxPushFrame(mux.0, &info, 1) } != ffi::WebPMuxError::WEBP_MUX_OK {
                return Err(fail("cannot add animation frame"));
            }
        }
        let mut data = Data(ffi::WebPData::default());
        if unsafe { ffi::WebPMuxAssemble(mux.0, &mut data.0) } != ffi::WebPMuxError::WEBP_MUX_OK
            || data.0.bytes.is_null()
        {
            return Err(fail("cannot assemble animation"));
        }
        Ok(unsafe { slice::from_raw_parts(data.0.bytes, data.0.size) }.to_vec())
    }
}

#[cfg(test)]
#[path = "native_raster_tests.rs"]
mod tests;
