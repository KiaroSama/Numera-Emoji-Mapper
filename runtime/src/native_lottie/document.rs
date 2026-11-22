use super::{
    LottieError, MAX_SOURCE_BYTES, MAX_TGS_BYTES, MAX_UNPACKED, Result,
    json_parser::Parser,
    json_value::{Node, Number, float_repr, integer_coercion},
};
use base64::{
    Engine as _, alphabet,
    engine::{
        DecodePaddingMode,
        general_purpose::{GeneralPurpose, GeneralPurposeConfig},
    },
};
use flate2::{Compression, GzBuilder, read::MultiGzDecoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    io::{Read, Write},
    path::Path,
};

pub(super) const MAX_IMAGE_ASSETS: usize = 64;
const MAX_ASSETS: usize = 4096;
const MAX_ASSET_PIXELS: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Document {
    root: Node,
}
impl Document {
    pub(super) fn preflight_image_assets(&self) -> Result<()> {
        let Some(Node::Array(assets)) = self.root.get("assets") else {
            return Ok(());
        };
        if assets.len() > MAX_ASSETS {
            return Err(LottieError::media("Lottie exceeds asset count limit"));
        }
        let (mut count, mut encoded, mut pixels) = (0, 0_u64, 0_u64);
        // rlottie's parser loads only the flat root assets array. PNG/JPEG payloads
        // cannot contain nested image references, so this walk has fixed depth.
        for asset in assets {
            let Node::Object(fields) = asset else {
                continue;
            };
            // The last p/layers field selects Image/Precomp in the native parser.
            let mut image = false;
            for (key, _) in fields {
                if key.iter().copied().eq("p".chars().map(u32::from)) {
                    image = true;
                } else if key.iter().copied().eq("layers".chars().map(u32::from)) {
                    image = false;
                }
            }
            if !image {
                continue;
            }
            count += 1;
            if count > MAX_IMAGE_ASSETS {
                return Err(LottieError::media("Lottie exceeds image asset count limit"));
            }
            let filename = asset.get("p").and_then(Node::text).ok_or_else(|| {
                LottieError::media("Lottie image asset path must be a Unicode string")
            })?;
            let embedded = match asset.get("e") {
                None => false,
                Some(Node::Number(Number::Integer(value))) => {
                    value.parse::<i32>().map(|value| value != 0).map_err(|_| {
                        LottieError::media(
                            "Lottie image asset embedded flag is outside integer range",
                        )
                    })?
                }
                _ => {
                    return Err(LottieError::media(
                        "Lottie image asset embedded flag must be an integer",
                    ));
                }
            };
            if !embedded {
                // Our C API passes resourcePath="". The native resource-path safety
                // check cannot establish that base and leaves external assets optional;
                // do not introduce JSON-parent or working-directory resolution here.
                continue;
            }
            if !filename.starts_with("data:") {
                continue;
            }
            let Some((_, payload)) = filename.split_once(',') else {
                continue;
            };
            let remaining_bytes = (MAX_SOURCE_BYTES as u64).saturating_sub(encoded);
            if payload.len() > MAX_UNPACKED
                || (payload.len() as u64).div_ceil(4) * 3 > remaining_bytes
            {
                return Err(LottieError::media(
                    "Lottie image assets exceed encoded byte limit",
                ));
            }
            // The native decoder accepts unpadded input and these URL/alternate
            // alphabet spellings. Check the same decoded bytes without narrowing
            // valid existing embedded images to only canonical base64 spelling.
            let payload = if payload
                .bytes()
                .any(|byte| matches!(byte, b'-' | b'.' | b'_' | b','))
            {
                Cow::Owned(
                    payload
                        .bytes()
                        .map(|byte| match byte {
                            b'-' | b'.' => b'+',
                            b'_' | b',' => b'/',
                            _ => byte,
                        })
                        .collect::<Vec<_>>(),
                )
            } else {
                Cow::Borrowed(payload.as_bytes())
            };
            let decoder = GeneralPurpose::new(
                &alphabet::STANDARD,
                GeneralPurposeConfig::new()
                    .with_decode_padding_mode(DecodePaddingMode::Indifferent)
                    .with_decode_allow_trailing_bits(true),
            );
            let data = decoder.decode(payload).map_err(|_| {
                LottieError::media("Lottie image asset contains invalid base64 data")
            })?;
            if !matches!(
                image::guess_format(&data),
                Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg)
            ) {
                return Err(LottieError::media(
                    "Lottie image asset must contain a PNG or JPEG image",
                ));
            }
            let remaining_pixels = MAX_ASSET_PIXELS.saturating_sub(pixels);
            let limits = crate::native_raster::DecodeLimits {
                max_encoded_bytes: remaining_bytes,
                max_pixels: remaining_pixels,
                max_decoded_bytes: remaining_pixels * 4,
                ..Default::default()
            };
            let decoded =
                crate::native_raster::decode_with_limits(&data, limits).map_err(|error| {
                    LottieError::media(format!(
                        "Lottie image asset exceeds decode bounds or is invalid: {error}"
                    ))
                })?;
            encoded += data.len() as u64;
            pixels += u64::from(decoded.rgba.width()) * u64::from(decoded.rgba.height());
        }
        Ok(())
    }

    pub fn from_bytes(raw: &[u8], filename: &str) -> Result<Self> {
        let decoded = if raw.starts_with(&[0x1f, 0x8b]) {
            let mut decoded = Vec::new();
            MultiGzDecoder::new(raw)
                .take(MAX_UNPACKED as u64 + 1)
                .read_to_end(&mut decoded)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::UnexpectedEof {
                        LottieError::new(
                            "EOFError",
                            "Compressed file ended before the end-of-stream marker was reached",
                        )
                    } else {
                        LottieError::io(error)
                    }
                })?;
            if decoded.len() > MAX_UNPACKED {
                return Err(LottieError::media(format!(
                    "{filename}: Lottie expands beyond {MAX_UNPACKED} bytes"
                )));
            }
            Cow::Owned(decoded)
        } else {
            if raw.len() > MAX_UNPACKED {
                return Err(LottieError::media(format!(
                    "{filename}: Lottie expands beyond {MAX_UNPACKED} bytes"
                )));
            }
            Cow::Borrowed(raw)
        };
        let text = std::str::from_utf8(&decoded).map_err(|error| {
            let mut result = LottieError::new("UnicodeDecodeError", error.to_string());
            result.raw_fallback = true;
            result.handled = true;
            result
        })?;
        let mut parser = Parser { text, at: 0 };
        if text.starts_with('\u{feff}') {
            return Err(parser.error("Unexpected UTF-8 BOM (decode using utf-8-sig)"));
        }
        let root = parser.node(0)?;
        parser.ws();
        if parser.at != text.len() {
            return Err(parser.error("Extra data"));
        }
        if !matches!(root, Node::Object(_)) {
            return Err(LottieError::media(format!(
                "{filename}: Lottie root is {}, expected an object",
                root.kind()
            )));
        }
        Ok(Self { root })
    }
    pub fn from_value(value: &Value) -> Result<Self> {
        Self::from_bytes(value.to_string().as_bytes(), "document")
    }
    pub fn to_value(&self) -> Result<Value> {
        // serde cannot represent nonfinite numbers or lone surrogates. Keep those in Document.
        serde_json::from_str(&self.json(false, false)).map_err(|error| {
            LottieError::new(
                "ValueError",
                format!("Lottie cannot be represented as standard JSON: {error}"),
            )
        })
    }
    pub fn json(&self, sorted: bool, ensure_ascii: bool) -> String {
        let mut out = String::new();
        self.root.write_json(&mut out, sorted, ensure_ascii);
        out
    }
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.json(true, true).into_bytes()
    }
    pub fn encode_gzip(&self, ensure_ascii: bool) -> Result<Vec<u8>> {
        let mut encoder = GzBuilder::new()
            .mtime(0)
            .operating_system(255)
            .write(Vec::new(), Compression::best());
        encoder
            .write_all(self.json(false, ensure_ascii).as_bytes())
            .map_err(LottieError::io)?;
        encoder.finish().map_err(LottieError::io)
    }
    pub fn validate(&self, filename: &str, compressed_size: u64) -> Result<()> {
        validate_size(filename, compressed_size)?;
        for key in ["v", "fr", "ip", "op", "layers"] {
            if self.root.get(key).is_none() {
                return Err(LottieError::media(format!(
                    "{filename}: Lottie missing required key '{key}'"
                )));
            }
        }
        let w = self.root.get("w").unwrap_or(&Node::Null);
        let h = self.root.get("h").unwrap_or(&Node::Null);
        if !w.is_512() || !h.is_512() {
            return Err(LottieError::media(format!(
                "{filename}: canvas {}x{}, expected 512x512",
                w.display(),
                h.display()
            )));
        }
        let originals = ["fr", "ip", "op"].map(|k| self.root.get(k).expect("required field"));
        let mut values = Vec::new();
        for value in originals {
            values.push(value.float().map_err(|error| {
                if matches!(error.kind.as_str(), "TypeError" | "ValueError") {
                    LottieError::media(format!("{filename}: non-numeric fr/ip/op ({error})"))
                } else {
                    error
                }
            })?);
        }
        for (name, value) in ["fr", "ip", "op"].into_iter().zip(&values) {
            if !value.is_finite() {
                return Err(LottieError::media(format!(
                    "{filename}: {name} is {}, not a finite number",
                    float_repr(*value)
                )));
            }
        }
        let (fr, ip, op) = (values[0], values[1], values[2]);
        if fr <= 0. {
            return Err(LottieError::media(format!(
                "{filename}: frame rate {} must be positive",
                float_repr(fr)
            )));
        }
        if fr > 60. {
            return Err(LottieError::media(format!(
                "{filename}: {} fps > 60 fps",
                float_repr(fr)
            )));
        }
        let duration = (op - ip) / fr;
        if duration <= 0. {
            return Err(LottieError::media(format!(
                "{filename}: empty timeline (ip={}, op={})",
                originals[1].display(),
                originals[2].display()
            )));
        }
        if duration > 3.01 {
            return Err(LottieError::media(format!(
                "{filename}: {duration:.2}s > 3.0s"
            )));
        }
        let mut layers = items(self.root.get("layers"))?;
        for asset in items(self.root.get("assets"))? {
            if matches!(asset, Node::Object(_)) {
                layers.extend(items(asset.get("layers"))?);
            }
        }
        for layer in layers {
            if !matches!(layer, Node::Object(_)) {
                continue;
            }
            for mask in items(layer.get("masksProperties"))? {
                if !matches!(mask, Node::Object(_)) {
                    return Err(LottieError::new(
                        "AttributeError",
                        format!("'{}' object has no attribute 'get'", mask.kind()),
                    ));
                }
                if mask.get("mode").and_then(Node::text).as_deref() == Some("s") {
                    let name = layer.get("nm").map_or_else(|| "'?'".into(), Node::repr);
                    return Err(LottieError::media(format!(
                        "{filename}: layer {name} uses a SUBTRACT mask, which Telegram refuses on upload (add masks are fine). Re-export without it."
                    )));
                }
            }
        }
        Ok(())
    }
    /// Apply tint to flat/keyframed colours and gradient colour stops, preserving alpha/opacity.
    pub fn tint(&mut self, rgb: [u8; 3]) {
        tint_node(&mut self.root, rgb.map(|c| c as f64 / 255.));
    }
    /// Package policy accepts int-coercible canvas fields, followed by strict TGS validation.
    pub fn package_bytes(&self, source_name: &str, output_name: &str) -> Result<Vec<u8>> {
        let integer = |key| -> Result<String> {
            self.root
                .get(key)
                .map_or_else(|| Ok("0".into()), integer_coercion)
        };
        let (w, h) = (integer("w")?, integer("h")?);
        if w != "512" || h != "512" {
            return Err(LottieError::media(format!(
                "{source_name}: animated emoji require a 512x512 Lottie canvas, got {w}x{h}. Re-export the animation at 512x512."
            )));
        }
        let bytes = self.encode_gzip(false)?;
        // Re-decode to enforce the same bounded expansion check even for plain JSON inputs.
        Self::from_bytes(&bytes, output_name)?.validate(output_name, bytes.len() as u64)?;
        Ok(bytes)
    }
}

fn items(value: Option<&Node>) -> Result<Vec<&Node>> {
    let Some(value) = value.filter(|v| v.truthy()) else {
        return Ok(Vec::new());
    };
    match value {
        Node::Array(items) => Ok(items.iter().collect()),
        // Iterating Python dictionaries/strings yields strings, ignored by the layer walker.
        Node::Object(items) => Ok(items.iter().map(|_| &STRING_NODE).collect()),
        Node::String(items) => Ok(items.iter().map(|_| &STRING_NODE).collect()),
        _ => Err(LottieError::new(
            "TypeError",
            format!("'{}' object is not iterable", value.kind()),
        )),
    }
}
static STRING_NODE: Node = Node::String(Vec::new());

fn tint_node(node: &mut Node, rgb: [f64; 3]) {
    fn numeric(values: &Node) -> bool {
        matches!(values,Node::Array(items) if items.iter().all(Node::numeric))
    }
    fn recolour(values: &mut Node, rgb: [f64; 3]) {
        if let Node::Array(items) = values {
            for (i, &c) in rgb.iter().take(items.len()).enumerate() {
                items[i] = Node::Number(Number::Float(c));
            }
        }
    }
    if let Some(colour) = node.get_mut("c").and_then(|v| v.get_mut("k")) {
        if numeric(colour) {
            recolour(colour, rgb);
        } else if let Node::Array(frames) = colour {
            for frame in frames {
                for key in ["s", "e"] {
                    if let Some(value) = frame.get_mut(key).filter(|v| numeric(v)) {
                        recolour(value, rgb);
                    }
                }
            }
        }
    }
    if let Some(gradient) = node.get_mut("g") {
        let count = gradient.get("p").and_then(|v| match v {
            Node::Number(Number::Integer(n)) if !n.starts_with('-') => {
                Some(n.parse::<usize>().unwrap_or(usize::MAX))
            }
            Node::Bool(b) => Some(usize::from(*b)),
            _ => None,
        });
        if let Some(Node::Array(items)) = gradient
            .get_mut("k")
            .and_then(|v| v.get_mut("k"))
            .filter(|v| numeric(v))
        {
            let count = count
                .unwrap_or_else(|| {
                    if items.len().is_multiple_of(4) {
                        items.len() / 4
                    } else {
                        0
                    }
                })
                .min(items.len() / 4);
            for base in (0..count * 4).step_by(4) {
                for (i, &c) in rgb.iter().enumerate() {
                    items[base + 1 + i] = Node::Number(Number::Float(c));
                }
            }
        }
    }
    match node {
        Node::Object(items) => {
            for (_, value) in items {
                tint_node(value, rgb);
            }
        }
        Node::Array(items) => {
            for value in items {
                tint_node(value, rgb);
            }
        }
        _ => {}
    }
}
fn validate_size(filename: &str, size: u64) -> Result<()> {
    if size > MAX_TGS_BYTES as u64 {
        Err(LottieError::media(format!(
            "{filename}: TGS {size} > {MAX_TGS_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}

fn source_limit_error(path: &Path) -> LottieError {
    LottieError::media(format!(
        "{}: Lottie source exceeds {MAX_SOURCE_BYTES} bytes",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn checked_file(
    path: &Path,
    limit: usize,
    oversized: &impl Fn(u64) -> LottieError,
) -> Result<std::fs::File> {
    let file = std::fs::File::open(path).map_err(LottieError::io)?;
    let size = file.metadata().map_err(LottieError::io)?.len();
    if size > limit as u64 {
        return Err(oversized(size));
    }
    Ok(file)
}

fn read_bounded(
    path: &Path,
    limit: usize,
    oversized: impl Fn(u64) -> LottieError,
) -> Result<Vec<u8>> {
    let file = checked_file(path, limit, &oversized)?;
    let mut bytes = Vec::new();
    // The read limit also applies if a file grows after its metadata was checked.
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(LottieError::io)?;
    if bytes.len() > limit {
        return Err(oversized(bytes.len() as u64));
    }
    Ok(bytes)
}

/// Bound encoded input before allocating, including the initial reencode read.
pub fn source_bytes(path: &Path) -> Result<Vec<u8>> {
    read_bounded(path, MAX_SOURCE_BYTES, |_| source_limit_error(path))
}

pub(super) fn hash_source(path: &Path) -> Result<sha2::digest::Output<Sha256>> {
    let mut reader = checked_file(path, MAX_SOURCE_BYTES, &|_| source_limit_error(path))?
        .take(MAX_SOURCE_BYTES as u64 + 1);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0;
    loop {
        let count = reader.read(&mut buffer).map_err(LottieError::io)?;
        if count == 0 {
            return Ok(digest.finalize());
        }
        total += count;
        if total > MAX_SOURCE_BYTES {
            return Err(source_limit_error(path));
        }
        digest.update(&buffer[..count]);
    }
}

pub fn load(path: &Path) -> Result<Document> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        let mut error = LottieError::new(
            "UnverifiedCodecError",
            "Lottie source must be an absolute regular non-symlink file",
        );
        error.caught_by_exception = false;
        return Err(error);
    }
    let raw = source_bytes(path)?;
    Document::from_bytes(
        &raw,
        &path.file_name().unwrap_or_default().to_string_lossy(),
    )
}
pub fn canonical_bytes(path: &Path) -> Result<Vec<u8>> {
    load(path).map(|d| d.canonical_bytes())
}
pub fn fingerprint(path: &Path) -> Result<Value> {
    let digest = match canonical_bytes(path) {
        Ok(bytes) => Sha256::digest(&bytes),
        Err(error) if error.raw_fallback => hash_source(path)?,
        Err(error) => return Err(error),
    };
    let hash = digest
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    Ok(
        json!({"content_key":format!("a:{}",&hash[..32]),"phash":null,"fmt":"animated","extension":".tgs"}),
    )
}
pub fn validate_file(path: &Path) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let size = std::fs::metadata(path).map_err(LottieError::io)?.len();
    validate_size(&name, size)?;
    let raw = read_bounded(path, MAX_TGS_BYTES, |size| {
        LottieError::media(format!("{name}: TGS {size} > {MAX_TGS_BYTES} bytes"))
    })?;
    if !raw.starts_with(&[0x1f, 0x8b]) {
        return Err(LottieError::media(format!(
            "{name}: TGS must be gzip-compressed Lottie"
        )));
    }
    let document = Document::from_bytes(&raw, &name).map_err(|error| {
        if error.raw_fallback {
            LottieError::media(format!("{name}: not a valid TGS/Lottie ({error})"))
        } else {
            error
        }
    })?;
    document.validate(&name, size)
}
