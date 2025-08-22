use windows::Win32::Foundation::{HANDLE, HMODULE};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D11_SRV_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::{Common::*, IDXGIDevice, IDXGIResource};
use windows::core::Interface;

pub fn create_d3d_device_on_same_adapter(
    existing_device: &ID3D11Device,
    enable_debug: bool,
) -> windows::core::Result<ID3D11Device> {
    let dxgi_device: IDXGIDevice = existing_device.cast()?;
    let adapter = unsafe { dxgi_device.GetAdapter()? };
    let feature_level: D3D_FEATURE_LEVEL = unsafe { existing_device.GetFeatureLevel() };

    let mut creation_flags = D3D11_CREATE_DEVICE_FLAG(0);
    if enable_debug {
        creation_flags |= D3D11_CREATE_DEVICE_DEBUG;
    }

    let feature_levels = [feature_level];
    let mut device: Option<ID3D11Device> = None;

    unsafe {
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE(std::ptr::null_mut()),
            creation_flags,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
    }
    Ok(device.unwrap())
}

/// Create a dynamic RGBA8 texture of the given size.
pub fn create_texture(device: &ID3D11Device, width: u32, height: u32) -> ID3D11Texture2D {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM, // BGR switches in flutter to RGB.. why xD
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DYNAMIC,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
        ..Default::default()
    };
    unsafe {
        let mut tex = None;
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .expect("CreateTexture2D failed");
        tex.unwrap()
    }
}

/// Create a ShaderResourceView for the given 2D texture.
pub fn create_srv(device: &ID3D11Device, texture: &ID3D11Texture2D) -> ID3D11ShaderResourceView {
    let mut tex_desc: D3D11_TEXTURE2D_DESC = unsafe { std::mem::zeroed() };
    unsafe {
        texture.GetDesc(&mut tex_desc);
    }

    let mut srv_desc: D3D11_SHADER_RESOURCE_VIEW_DESC = unsafe { std::mem::zeroed() };
    srv_desc.Format = tex_desc.Format;
    srv_desc.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
    srv_desc.Anonymous.Texture2D.MipLevels = tex_desc.MipLevels;
    srv_desc.Anonymous.Texture2D.MostDetailedMip = 0;

    unsafe {
        let mut view: Option<ID3D11ShaderResourceView> = None;
        device
            .CreateShaderResourceView(texture, Some(&srv_desc), Some(&mut view))
            .expect("CreateShaderResourceView failed");
        view.unwrap()
    }
}

pub fn create_shared_texture_and_get_handle(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Result<(ID3D11Texture2D, HANDLE), String> {
    unsafe {
        let texture_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,

            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32 | D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_SHARED.0 as u32,
        };

        let mut texture_opt: Option<ID3D11Texture2D> = None;

        device
            .CreateTexture2D(&texture_desc, None, Some(&mut texture_opt))
            .map_err(|e| format!("Failed to create shared texture: {}", e))?;

        let texture = texture_opt.unwrap();

        let resource: IDXGIResource = texture
            .cast()
            .map_err(|e| format!("Failed to cast texture to IDXGIResource: {}", e))?;

        let handle = resource
            .GetSharedHandle()
            .map_err(|e| format!("Failed to get shared handle: {}", e))?;

        Ok((texture, handle))
    }
}

pub fn create_compositing_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> ID3D11Texture2D {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    unsafe {
        let mut tex = None;
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .expect("CreateTexture2D for compositing texture failed");
        tex.unwrap()
    }
}
