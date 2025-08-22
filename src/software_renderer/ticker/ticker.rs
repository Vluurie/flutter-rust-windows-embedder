use log::error;
use std::{mem, ptr};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_MAP_WRITE_DISCARD, D3D11_MAPPED_SUBRESOURCE, ID3D11DeviceContext,
};

use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;

pub fn tick(overlay: &FlutterOverlay, context: &ID3D11DeviceContext) {
    // Only proceed if we are in software rendering mode (pixel_buffer exists)
    if let Some(pixel_buffer) = &overlay.pixel_buffer {
        if overlay.width == 0 || overlay.height == 0 {
            return;
        }

        unsafe {
            let mut mapped: D3D11_MAPPED_SUBRESOURCE = mem::zeroed();
            if context
                .Map(
                    &overlay.texture,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped),
                )
                .is_ok()
            {
                let data = mapped.pData;
                if data.is_null() {
                    error!("[tick] mapped pData is null");
                    context.Unmap(&overlay.texture, 0);
                    return;
                }

                let rp_tex = mapped.RowPitch as usize;
                let rp_buf = (overlay.width as usize) * 4;

                if rp_tex < rp_buf {
                    error!("[tick] tex_pitch {} < buf_pitch {}", rp_tex, rp_buf);
                    context.Unmap(&overlay.texture, 0);
                    return;
                }

                if pixel_buffer.len() < rp_buf * (overlay.height as usize) {
                    error!(
                        "[tick] buffer too small ({} req, {} have)",
                        rp_buf * (overlay.height as usize),
                        pixel_buffer.len()
                    );
                    context.Unmap(&overlay.texture, 0);
                    return;
                }

                let src = pixel_buffer.as_ptr();
                for y in 0..overlay.height as usize {
                    let dst = (data as *mut u8).add(y * rp_tex);
                    let row = src.add(y * rp_buf);
                    ptr::copy_nonoverlapping(row, dst, rp_buf);
                }

                context.Unmap(&overlay.texture, 0);
            }
        }
    }
}
