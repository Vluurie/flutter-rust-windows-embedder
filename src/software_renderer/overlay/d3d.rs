use windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

/// Create a dynamic RGBA8 texture of the given size.
pub fn create_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> ID3D11Texture2D {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM, // BGR switches in flutter to RGB.. why xD
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
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
pub fn create_srv(
    device: &ID3D11Device,
    texture: &ID3D11Texture2D,
) -> ID3D11ShaderResourceView {
    let mut tex_desc: D3D11_TEXTURE2D_DESC = unsafe { std::mem::zeroed() };
    unsafe {
        texture.GetDesc(&mut tex_desc);
    }

    let mut srv_desc: D3D11_SHADER_RESOURCE_VIEW_DESC =
        unsafe { std::mem::zeroed() };
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
