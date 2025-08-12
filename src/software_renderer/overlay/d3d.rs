use windows::Win32::Foundation::{GENERIC_ALL, HANDLE};
use windows::Win32::Graphics::Direct3D::{D3D_FEATURE_LEVEL_11_1, D3D11_SRV_DIMENSION_TEXTURE2D};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::{
    Common::*, DXGI_SHARED_RESOURCE_READ, DXGI_SHARED_RESOURCE_WRITE, IDXGIDevice, IDXGIResource,
    IDXGIResource1,
};
use windows::core::{Interface, PCWSTR};

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
) -> windows::core::Result<(ID3D11Texture2D, HANDLE)> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: D3D11_RESOURCE_MISC_SHARED.0 as u32,
    };

    unsafe {
        let mut tex = None;
        device.CreateTexture2D(&desc, None, Some(&mut tex))?;

        let texture = tex.unwrap();
        let resource: IDXGIResource = texture.cast()?;
        let handle = resource.GetSharedHandle()?;
        Ok((texture, handle))
    }
}
pub fn log_device_adapter_info(device: &ID3D11Device) {
    println!("[DXGI PROBE] Querying adapter info for device {:p}", device);

    if let Ok(dxgi_device) = device.cast::<IDXGIDevice>() {
        if let Ok(adapter) = unsafe { dxgi_device.GetAdapter() } {
            if let Ok(desc) = unsafe { adapter.GetDesc() } {
                let description_raw = &desc.Description[..];
                let null_terminator_pos = description_raw
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(description_raw.len());
                let description = String::from_utf16_lossy(&description_raw[..null_terminator_pos]);

                println!(
                    "[DXGI PROBE]   -> SUCCESS: Device is running on GPU: {}",
                    description
                );
            } else {
                println!("[DXGI PROBE]   -> ERROR: Failed to get adapter description.");
            }
        } else {
            println!("[DXGI PROBE]   -> ERROR: Failed to get adapter from DXGI device.");
        }
    } else {
        println!("[DXGI PROBE]   -> ERROR: Failed to cast ID3D11Device to IDXGIDevice.");
    }
}

pub fn log_texture_properties(texture: &ID3D11Texture2D) {
    println!("[DXGI TEXTURE PROBE] Inspecting D3D11 texture properties...");

    let mut desc: windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC = Default::default();
    unsafe { texture.GetDesc(&mut desc) };

    // Gib die grundlegenden Eigenschaften aus
    println!("    - Dimensions: {}x{}", desc.Width, desc.Height);
    println!("    - Format: {:?}", desc.Format);
    println!("    - MipLevels: {}", desc.MipLevels);

    // Gib die Usage-Flags aus
    let usage = match desc.Usage {
        windows::Win32::Graphics::Direct3D11::D3D11_USAGE_DEFAULT => "DEFAULT (GPU read/write)",
        windows::Win32::Graphics::Direct3D11::D3D11_USAGE_IMMUTABLE => "IMMUTABLE (GPU read-only)",
        windows::Win32::Graphics::Direct3D11::D3D11_USAGE_DYNAMIC => {
            "DYNAMIC (CPU write, GPU read)"
        }
        windows::Win32::Graphics::Direct3D11::D3D11_USAGE_STAGING => "STAGING (CPU read/write)",
        _ => "Unknown",
    };
    println!("    - Usage: {}", usage);

    // Gib die Bind-Flags aus
    let mut bind_flags = Vec::new();
    if (desc.BindFlags & windows::Win32::Graphics::Direct3D11::D3D11_BIND_SHADER_RESOURCE.0 as u32)
        != 0
    {
        bind_flags.push("SHADER_RESOURCE");
    }
    if (desc.BindFlags & windows::Win32::Graphics::Direct3D11::D3D11_BIND_RENDER_TARGET.0 as u32)
        != 0
    {
        bind_flags.push("RENDER_TARGET");
    }
    if (desc.BindFlags & windows::Win32::Graphics::Direct3D11::D3D11_BIND_DEPTH_STENCIL.0 as u32)
        != 0
    {
        bind_flags.push("DEPTH_STENCIL");
    }
    if (desc.BindFlags & windows::Win32::Graphics::Direct3D11::D3D11_BIND_UNORDERED_ACCESS.0 as u32)
        != 0
    {
        bind_flags.push("UNORDERED_ACCESS");
    }
    println!("    - BindFlags: [{}]", bind_flags.join(", "));

    // Gib die CPU-Access-Flags aus
    let mut cpu_flags = Vec::new();
    if (desc.CPUAccessFlags & windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_WRITE.0 as u32)
        != 0
    {
        cpu_flags.push("CPU_WRITE");
    }
    if (desc.CPUAccessFlags & windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_READ.0 as u32)
        != 0
    {
        cpu_flags.push("CPU_READ");
    }
    if cpu_flags.is_empty() {
        cpu_flags.push("NONE");
    }
    println!("    - CPUAccessFlags: [{}]", cpu_flags.join(", "));

    // Gib die entscheidenden Misc-Flags aus
    let mut misc_flags = Vec::new();
    if (desc.MiscFlags & windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_SHARED.0 as u32)
        != 0
    {
        misc_flags.push("SHARED");
    }
    if (desc.MiscFlags
        & windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32)
        != 0
    {
        misc_flags.push("SHARED_KEYEDMUTEX");
    }
    if (desc.MiscFlags
        & windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_GDI_COMPATIBLE.0 as u32)
        != 0
    {
        misc_flags.push("GDI_COMPATIBLE");
    }
    if (desc.MiscFlags
        & windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32)
        != 0
    {
        misc_flags.push("GENERATE_MIPS");
    }
    if misc_flags.is_empty() {
        misc_flags.push("NONE");
    }
    println!("    - MiscFlags: [{}]", misc_flags.join(", "));
}

pub fn log_device_feature_level(device: &ID3D11Device) {
    println!(
        "[DXGI PROBE] Querying feature level for device {:p}",
        device
    );
    let feature_level = unsafe { device.GetFeatureLevel() };

    let level_str = match feature_level {
        D3D_FEATURE_LEVEL_12_1 => "12.1",
        D3D_FEATURE_LEVEL_12_0 => "12.0",
        D3D_FEATURE_LEVEL_11_1 => "11.1",
        D3D_FEATURE_LEVEL_11_0 => "11.0",
        D3D_FEATURE_LEVEL_10_1 => "10.1",
        D3D_FEATURE_LEVEL_10_0 => "10.0",
        D3D_FEATURE_LEVEL_9_3 => "9.3",
        D3D_FEATURE_LEVEL_9_2 => "9.2",
        D3D_FEATURE_LEVEL_9_1 => "9.1",
        _ => "Unknown or older",
    };

    println!(
        "[DXGI PROBE]   -> SUCCESS: Device is running with Feature Level: {}",
        level_str
    );
}

pub fn log_device_creation_flags(flags: D3D11_CREATE_DEVICE_FLAG) {
    println!("[DXGI PROBE] Überprüfe D3D11 Device Creation Flags...");

    let mut set_flags = Vec::new();

    if (flags & D3D11_CREATE_DEVICE_SINGLETHREADED).0 != 0 {
        set_flags.push("SINGLETHREADED");
    }
    if (flags & D3D11_CREATE_DEVICE_DEBUG).0 != 0 {
        set_flags.push("DEBUG");
    }
    if (flags & D3D11_CREATE_DEVICE_SWITCH_TO_REF).0 != 0 {
        set_flags.push("SWITCH_TO_REF");
    }
    if (flags & D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS).0 != 0 {
        set_flags.push("PREVENT_INTERNAL_THREADING_OPTIMIZATIONS");
    }
    if (flags & D3D11_CREATE_DEVICE_BGRA_SUPPORT).0 != 0 {
        set_flags.push("BGRA_SUPPORT");
    }
    if (flags & D3D11_CREATE_DEVICE_DEBUGGABLE).0 != 0 {
        set_flags.push("DEBUGGABLE");
    }
    if (flags & D3D11_CREATE_DEVICE_DISABLE_GPU_TIMEOUT).0 != 0 {
        set_flags.push("DISABLE_GPU_TIMEOUT");
    }
    if (flags & D3D11_CREATE_DEVICE_VIDEO_SUPPORT).0 != 0 {
        set_flags.push("VIDEO_SUPPORT");
    }

    if set_flags.is_empty() {
        println!("[DXGI PROBE]   -> Keine Flags gesetzt.");
    } else {
        println!(
            "[DXGI PROBE]   -> Gesetzte Flags: [{}]",
            set_flags.join(", ")
        );
    }
}
