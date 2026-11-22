use super::{
    DecodeLimits, DecodedRaster, RasterAnimation, RasterError, RasterFrame, Result, WebpOptions,
};
use image::RgbaImage;
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
    let mut picture = Picture(ffi::WebPPicture::new().map_err(|_| fail("picture ABI mismatch"))?);
    picture.0.width = rgba.width() as i32;
    picture.0.height = rgba.height() as i32;
    picture.0.use_argb = 1;
    if unsafe { ffi::WebPPictureImportRGBA(&mut picture.0, rgba.as_ptr(), rgba.width() as i32 * 4) }
        == 0
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
        if unsafe { ffi::WebPAnimEncoderAdd(encoder.0, &mut picture.0, timestamp, &configuration) }
            == 0
        {
            return Err(fail("cannot encode animation frame"));
        }
        timestamp = timestamp
            .checked_add(frame.duration_ms as i32)
            .ok_or_else(|| fail("animation timeline exceeds format limits"))?;
    }
    // The terminal timestamp determines the last frame's duration; omitting
    // this call truncates clips whose final duration differs from the prior one.
    if unsafe { ffi::WebPAnimEncoderAdd(encoder.0, ptr::null_mut(), timestamp, ptr::null()) } == 0 {
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
    if unsafe { ffi::WebPMuxSetAnimationParams(mux.0, &params) } != ffi::WebPMuxError::WEBP_MUX_OK {
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
