use super::{DecodeLimits, DecodedRaster, RasterError, Result};
use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
use sha2::{Digest, Sha256};

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

pub(super) fn multiply_rounded(value: u8, alpha: u8) -> u8 {
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
