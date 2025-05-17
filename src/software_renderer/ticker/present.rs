use log::error;
use std::{ptr};

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

    if allocation.is_null() {
        error!("on_present: allocation is null");
        return true;
    }
    if ov.width == 0 || ov.height == 0 || ov.pixel_buffer.is_empty() {
        error!(
            "on_present: invalid overlay (w={}, h={}, buf={})",
            ov.width,
            ov.height,
            ov.pixel_buffer.len()
        );
        return true;
    }

    let pitch = (ov.width as usize) * 4;
    let rows  = std::cmp::min(height_flutter, ov.height as usize);
    let bytes = std::cmp::min(row_bytes_flutter, pitch);
    if rows == 0 || bytes == 0 {
        return true;
    }

    let src_base = allocation as *const u8;
    let dst_base = ov.pixel_buffer.as_mut_ptr();
    for y in 0..rows {
        unsafe {
            let src = src_base.add(y * row_bytes_flutter);
            let dst = dst_base.add(y * pitch);
            ptr::copy_nonoverlapping(src, dst, bytes);
        }
    }

    true
}