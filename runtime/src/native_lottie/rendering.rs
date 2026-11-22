use super::{Document, LottieError, Result, load};
use image::RgbaImage;
use sha2::{Digest, Sha256};
use std::{
    ffi::{CString, c_char, c_void},
    path::Path,
    ptr::NonNull,
};

unsafe extern "C" {
    fn lottie_animation_from_data(
        data: *const c_char,
        key: *const c_char,
        resources: *const c_char,
    ) -> *mut c_void;
    fn lottie_animation_destroy(animation: *mut c_void);
    fn lottie_animation_get_size(animation: *const c_void, width: *mut usize, height: *mut usize);
    fn lottie_animation_get_duration(animation: *const c_void) -> f64;
    fn lottie_animation_get_totalframe(animation: *const c_void) -> usize;
    fn lottie_animation_get_framerate(animation: *const c_void) -> f64;
    fn lottie_animation_get_frame_at_pos(animation: *const c_void, position: f32) -> usize;
    fn lottie_animation_render(
        animation: *mut c_void,
        frame: usize,
        buffer: *mut u32,
        width: usize,
        height: usize,
        stride: usize,
    );
}

/// Owns one engine animation; calls are synchronous and cannot outlive the instance.
pub struct Animation {
    handle: NonNull<c_void>,
}
impl Drop for Animation {
    fn drop(&mut self) {
        unsafe {
            lottie_animation_destroy(self.handle.as_ptr());
        }
    }
}
impl Animation {
    pub fn from_document(document: &Document) -> Result<Self> {
        document.preflight_image_assets()?;
        let text = CString::new(document.json(false, true))
            .map_err(|_| LottieError::new("ValueError", "Lottie JSON contains a NUL byte"))?;
        // Hash the data as its cache key, never reuse the wrapper's empty key across documents.
        let key = CString::new(
            Sha256::digest(text.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
        .expect("hex key");
        // Native loadFromData copies its strings; all returned pixel storage is separately owned.
        let pointer =
            unsafe { lottie_animation_from_data(text.as_ptr(), key.as_ptr(), c"".as_ptr()) };
        let handle = NonNull::new(pointer).ok_or_else(|| {
            LottieError::new("RuntimeError", "rlottie could not load the Lottie document")
        })?;
        let result = Self { handle };
        if !result.frame_rate().is_finite()
            || result.frame_rate() <= 0.
            || !result.duration().is_finite()
            || result.total_frames() == 0
        {
            return Err(LottieError::new(
                "RuntimeError",
                "rlottie animation has an invalid or empty timeline",
            ));
        }
        Ok(result)
    }
    pub fn from_path(path: &Path) -> Result<Self> {
        Self::from_document(&load(path)?)
    }
    pub fn frame_rate(&self) -> f64 {
        unsafe { lottie_animation_get_framerate(self.handle.as_ptr()) }
    }
    pub fn total_frames(&self) -> usize {
        unsafe { lottie_animation_get_totalframe(self.handle.as_ptr()) }
    }
    pub fn duration(&self) -> f64 {
        unsafe { lottie_animation_get_duration(self.handle.as_ptr()) }
    }
    pub fn size(&self) -> (usize, usize) {
        let (mut w, mut h) = (0, 0);
        unsafe {
            lottie_animation_get_size(self.handle.as_ptr(), &mut w, &mut h);
        }
        (w, h)
    }
    pub fn frame_at_pos(&self, pos: f64) -> Result<usize> {
        if !pos.is_finite() || !(0.0..=1.0).contains(&pos) {
            return Err(LottieError::new(
                "ValueError",
                "Lottie position must be finite and between 0 and 1",
            ));
        }
        Ok(unsafe { lottie_animation_get_frame_at_pos(self.handle.as_ptr(), pos as f32) })
    }
    /// Frame is relative to the animation in-point. The historical native C API uses integer frames.
    pub fn render_rgba(&mut self, frame: f64, width: u32, height: u32) -> Result<RgbaImage> {
        if !frame.is_finite()
            || frame < 0.
            || frame >= self.total_frames() as f64
            || width == 0
            || height == 0
        {
            return Err(LottieError::new(
                "ValueError",
                "invalid Lottie frame or output dimensions",
            ));
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or_else(|| {
                LottieError::new("ValueError", "Lottie output dimensions are too large")
            })?;
        let mut surface = vec![0_u32; pixels];
        // SAFETY: surface owns width*height aligned pixels; stride and all dimensions match.
        unsafe {
            lottie_animation_render(
                self.handle.as_ptr(),
                frame as usize,
                surface.as_mut_ptr(),
                width as usize,
                height as usize,
                width as usize * 4,
            );
        }
        let mut rgba = Vec::with_capacity(pixels * 4);
        // rlottie emits native-endian ARGB32. The historical wrapper interprets those channels
        // as BGRA without unpremultiplication; preserving that step keeps preview appearance.
        for p in surface {
            rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
        }
        RgbaImage::from_raw(width, height, rgba)
            .ok_or_else(|| LottieError::new("RuntimeError", "invalid rlottie raster buffer"))
    }
}
