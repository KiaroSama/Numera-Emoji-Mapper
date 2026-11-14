//! Bounded SVG rendering, including compressed documents and referenced images.
use crate::{codec_error::CodecError, native_raster};
use image::RgbaImage;
use std::{
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

const MAX_SVG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_REFERENCED_IMAGES: usize = 64;
const MAX_REFERENCE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_REFERENCE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_SVG_IMAGE_DEPTH: usize = 8;

#[derive(Default)]
struct References {
    count: usize,
    encoded: u64,
    pixels: u64,
    depth: usize,
    error: Option<String>,
}
impl References {
    fn refuse(&mut self, message: impl Into<String>) -> Option<resvg::usvg::ImageKind> {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
        None
    }
    fn limits(&self) -> native_raster::DecodeLimits {
        native_raster::DecodeLimits {
            max_encoded_bytes: MAX_REFERENCE_BYTES.saturating_sub(self.encoded),
            max_pixels: MAX_REFERENCE_PIXELS.saturating_sub(self.pixels),
            max_decoded_bytes: (MAX_REFERENCE_PIXELS.saturating_sub(self.pixels) * 4)
                .min(native_raster::DecodeLimits::default().max_decoded_bytes),
            ..Default::default()
        }
    }
    fn image(&mut self, data: Arc<Vec<u8>>) -> Option<resvg::usvg::ImageKind> {
        if self.error.is_some() {
            return None;
        }
        if self.count >= MAX_REFERENCED_IMAGES {
            return self.refuse("SVG exceeds referenced-image count limit");
        }
        let format = match image::guess_format(&data) {
            Ok(
                format @ (image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::Gif
                | image::ImageFormat::WebP),
            ) => format,
            _ => return None,
        };
        let decoded = match native_raster::decode_with_limits(&data, self.limits()) {
            Ok(decoded) => decoded,
            Err(error) if is_bound_error(&error.to_string()) => {
                return self.refuse(format!(
                    "SVG referenced image exceeds decode bounds or is invalid: {error}"
                ));
            }
            Err(_) => return None,
        };
        self.count += 1;
        self.encoded += data.len() as u64;
        self.pixels += u64::from(decoded.rgba.width()) * u64::from(decoded.rgba.height());
        // The renderer receives only previously decoded, dimension-checked bytes.
        // Keeping the original raster format preserves its historical rendering.
        Some(match format {
            image::ImageFormat::Png => resvg::usvg::ImageKind::PNG(data),
            image::ImageFormat::Jpeg => resvg::usvg::ImageKind::JPEG(data),
            image::ImageFormat::Gif => resvg::usvg::ImageKind::GIF(data),
            image::ImageFormat::WebP => resvg::usvg::ImageKind::WEBP(data),
            _ => unreachable!("formats checked above"),
        })
    }
}

fn is_bound_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    ["limit", "exceed", "overflow", "too large"]
        .iter()
        .any(|word| message.contains(word))
}

fn nested_svg(
    data: &[u8],
    options: &resvg::usvg::Options<'_>,
    references: &Arc<Mutex<References>>,
) -> Option<resvg::usvg::ImageKind> {
    let decoded = match svg_bytes(data) {
        Ok(decoded) => decoded,
        Err(error) if is_bound_error(&error.message) => {
            return references
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .refuse(error.message);
        }
        Err(_) => return None,
    };
    {
        let mut references = references.lock().unwrap_or_else(|error| error.into_inner());
        if references.error.is_some() {
            return None;
        }
        if references.depth >= MAX_SVG_IMAGE_DEPTH {
            return references.refuse("SVG referenced image nesting exceeds depth limit");
        }
        if references.count >= MAX_REFERENCED_IMAGES {
            return references.refuse("SVG exceeds referenced-image count limit");
        }
        if decoded.len() as u64 > MAX_REFERENCE_BYTES.saturating_sub(references.encoded) {
            return references.refuse("SVG referenced documents exceed aggregate input byte limit");
        }
        references.count += 1;
        references.encoded += decoded.len() as u64;
        references.depth += 1;
    }
    // Nested parsing can call our data resolver again. Never retain the budget
    // mutex across that call; reserve before recursing and release depth after it.
    // from_data_nested ignores external references inside a nested SVG per SVG's
    // image semantics while forwarding the bounded data resolver.
    let tree = resvg::usvg::Tree::from_data_nested(&decoded, options).ok();
    let mut references = references.lock().unwrap_or_else(|error| error.into_inner());
    references.depth -= 1;
    if references.error.is_some() {
        None
    } else {
        tree.map(resvg::usvg::ImageKind::SVG)
    }
}

fn svg_bytes(data: &[u8]) -> Result<Vec<u8>, CodecError> {
    if data.starts_with(&[0x1f, 0x8b]) {
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(data)
            .take(MAX_SVG_BYTES + 1)
            .read_to_end(&mut decoded)
            .map_err(|_| CodecError::media("invalid SVGZ stream"))?;
        if decoded.len() as u64 > MAX_SVG_BYTES {
            return Err(CodecError::media(
                "SVG decoded document exceeds 8 MiB limit",
            ));
        }
        Ok(decoded)
    } else if data.len() as u64 > MAX_SVG_BYTES {
        Err(CodecError::media(
            "SVG decoded document exceeds 8 MiB limit",
        ))
    } else {
        Ok(data.to_vec())
    }
}

pub fn load_rgba(path: &Path) -> Result<RgbaImage, CodecError> {
    let encoded = native_raster::read_bounded(path, Default::default())?;
    // Do not pass compressed data to usvg's unbounded SVGZ convenience decoder.
    let data = svg_bytes(&encoded)?;
    let failed = || {
        CodecError::media(format!(
            "failed to render SVG: {}",
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
        ))
    };
    let text = std::str::from_utf8(&data).map_err(|_| failed())?;
    let references = Arc::new(Mutex::new(References::default()));
    let data_references = references.clone();
    let path_references = references.clone();
    let resolver = resvg::usvg::ImageHrefResolver {
        resolve_data: Box::new(move |mime, data, options| {
            if mime == "image/svg+xml"
                || (mime == "text/plain" && image::guess_format(&data).is_err())
            {
                return nested_svg(&data, options, &data_references);
            }
            match mime {
                "image/png" | "image/jpg" | "image/jpeg" | "image/gif" | "image/webp"
                | "text/plain" => data_references
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .image(data),
                _ => None,
            }
        }),
        resolve_string: Box::new(move |href, options| {
            let mut references = path_references
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if references.error.is_some() {
                return None;
            }
            let path = options.get_abs_path(Path::new(href));
            let is_svg = path.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("svg") || extension.eq_ignore_ascii_case("svgz")
            });
            if !std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
                return None;
            }
            match native_raster::read_bounded(&path, references.limits()) {
                Ok(data) if is_svg => {
                    drop(references);
                    nested_svg(&data, options, &path_references)
                }
                Ok(data) => references.image(Arc::new(data)),
                Err(error) if is_bound_error(&error.to_string()) => references.refuse(format!(
                    "SVG referenced image exceeds input bounds or cannot be read: {error}"
                )),
                Err(_) => None,
            }
        }),
    };
    let mut options = resvg::usvg::Options {
        // resvg-py's historical font-size default differs from usvg's default.
        font_size: 16.0,
        resources_dir: path.parent().map(Path::to_path_buf),
        image_href_resolver: resolver,
        ..Default::default()
    };
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(text, &options).map_err(|_| failed())?;
    if let Some(error) = references
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .error
        .clone()
    {
        return Err(CodecError::media(error));
    }
    let size = tree.size();
    let scale = 256.0 / size.width();
    let height = (size.height() * scale).ceil().max(1.0);
    if !height.is_finite() || height > 16_384.0 {
        return Err(CodecError::media("SVG raster exceeds dimension limit"));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(256, height as u32)
        .ok_or_else(|| CodecError::media("SVG raster exceeds allocation limit"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap
        .encode_png()
        .map_err(|error| CodecError::media(error.to_string()))?;
    Ok(native_raster::decode(&png)?.rgba)
}
