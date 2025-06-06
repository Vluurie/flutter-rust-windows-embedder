use log::error;
use std::{mem, ptr};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_MAP_WRITE_DISCARD, D3D11_MAPPED_SUBRESOURCE, ID3D11DeviceContext,
};

use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;

pub fn tick(overlay: &FlutterOverlay , context: &ID3D11DeviceContext) {
    if overlay.width == 0 || overlay.height == 0 {
        return;
    }

    unsafe {
        let mut mapped: D3D11_MAPPED_SUBRESOURCE = mem::zeroed();
        match context.Map(
            &overlay.texture,
            0,
            D3D11_MAP_WRITE_DISCARD,
            0,
            Some(&mut mapped),
        ) {
            Ok(_) => {
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

                if overlay.pixel_buffer.len() < rp_buf * (overlay.height as usize) {
                    error!(
                        "[tick] buffer too small ({} req, {} have)",
                        rp_buf * (overlay.height as usize),
                        overlay.pixel_buffer.len()
                    );
                    context.Unmap(&overlay.texture, 0);
                    return;
                }

                let src = overlay.pixel_buffer.as_ptr();
                for y in 0..overlay.height as usize {
                    let dst = (data as *mut u8).add(y * rp_tex);
                    let row = src.add(y * rp_buf);
                    ptr::copy_nonoverlapping(row, dst, rp_buf);
                }

                context.Unmap(&overlay.texture, 0);
            }
            Err(e) => {
                error!("[tick] failed to map texture: {:?}", e);
            }
        }
    }
}
