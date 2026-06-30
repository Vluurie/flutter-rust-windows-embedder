use log::error;
use std::sync::atomic::Ordering;

use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;

pub extern "C" fn on_present(
    user_data: *mut std::ffi::c_void,
    allocation: *const std::ffi::c_void,
    row_bytes_flutter: usize,
    height_flutter: usize,
) -> bool {
    if user_data.is_null() {
        error!("on_present: user_data is null");
        return true;
    }
    let ov = unsafe { &mut *(user_data as *mut FlutterOverlay) };

    if let Some(pixel_buffer) = ov.pixel_buffer.as_mut() {
        if allocation.is_null() {
            error!("on_present: allocation is null");
            return true;
        }
        if ov.width == 0 || ov.height == 0 || pixel_buffer.is_empty() {
            error!(
                "on_present: invalid overlay (w={}, h={}, buf={})",
                ov.width,
                ov.height,
                pixel_buffer.len()
            );
            return true;
        }

        let pitch = (ov.width as usize) * 4;
        let rows = std::cmp::min(height_flutter, ov.height as usize);
        let bytes = std::cmp::min(row_bytes_flutter, pitch);
        if rows == 0 || bytes == 0 {
            return true;
        }

        let src_len = rows.saturating_sub(1) * row_bytes_flutter + bytes;
        let dst_len = pixel_buffer.len();
        let src = unsafe { std::slice::from_raw_parts(allocation as *const u8, src_len) };
        copy_framebuffer(src, pixel_buffer, rows, row_bytes_flutter, pitch, bytes, dst_len);

        ov.software_frame_dirty.store(true, Ordering::Release);
        ov.software_first_frame_rendered.store(true, Ordering::Release);
    }

    true
}

/// Copies `rows` rows of `bytes` bytes each from a Flutter framebuffer (`src_pitch`
/// stride) into a destination pixel buffer (`dst_pitch` stride). Bounds-checked so
/// a short buffer cannot overrun. Pure and unit-testable.
pub(crate) fn copy_framebuffer(
    src: &[u8],
    dst: &mut [u8],
    rows: usize,
    src_pitch: usize,
    dst_pitch: usize,
    bytes: usize,
    dst_capacity: usize,
) {
    for y in 0..rows {
        let s = y * src_pitch;
        let d = y * dst_pitch;
        if s + bytes > src.len() || d + bytes > dst_capacity || d + bytes > dst.len() {
            break;
        }
        dst[d..d + bytes].copy_from_slice(&src[s..s + bytes]);
    }
}
