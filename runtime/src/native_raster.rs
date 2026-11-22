//! First-party raster operations. No interpreter, subprocess, or Python ABI is used here.
//!
//! Identity uses Pillow's 22-bit separable convolution and 8-bit alpha arithmetic. In
//! particular, replacing this with `image::imageops::resize` changes catalogue keys.
//! PNG and WebP *encoded bytes* may differ from historical Pillow releases; lossless
//! decoded pixels and the existing normalized content identity remain the contract.
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageEncoder, Rgba, RgbaImage};
use std::fmt;
use std::io::{Cursor, Read};
use std::path::Path;

#[path = "native_raster/pixels.rs"]
mod pixels;
// The only unsafe boundary is the bundled native WebP codec. Each allocation is
// owned by an RAII guard; input/output buffers remain alive for the entire FFI call.
#[allow(unsafe_code)]
#[path = "native_raster/webp.rs"]
mod webp;

use pixels::multiply_rounded;
pub use pixels::{
    ResampleFilter, alpha_bounds, audit_signature, blank_rgba, close_pixels, content_key, dhash,
    dhash_mode, digest_hex, fit_canvas, frame_phash, is_blank, logo_distance, normalized_rgba,
    on_white, premultiplied_pixels, remap_signature, resize_lanczos, resize_rgba, same_decoded,
    same_frame, same_rgba, source_dhash, thumbnail, tint,
};

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

#[cfg(test)]
#[path = "native_raster_tests.rs"]
mod tests;
