//! A minimal **software**-renderer Flutter embedder that writes into a D3D11 texture.
//!
//! ## Crate Requirements
//! - A `embedder` module generated via bindgen from `flutter_embedder.h`.
//! - `path_utils::{get_flutter_paths, get_flutter_paths_from}` for locating assets.
//! - (Plugin loading via `plugin_loader` is **not** supported in this low-level embedder.)

use std::{
    ffi::{CString, OsString, c_void},
    mem,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
};
use windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_WRITE, D3D11_MAP_WRITE_DISCARD,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DYNAMIC, ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};

use crate::{embedder, path_utils};

/// Must match C `#define FLUTTER_ENGINE_VERSION 1`.
const FLUTTER_ENGINE_VERSION: usize = 1;

// Aliases for our bindgen-generated types:
type FlutterEngineRef = embedder::FlutterEngine;
type FlutterProjectArgs = embedder::FlutterProjectArgs;
type FlutterSoftwareRendererConfig = embedder::FlutterSoftwareRendererConfig;
type FlutterRendererConfig = embedder::FlutterRendererConfig;

/// Called by Flutter each time a new RGBA frame is ready.
/// Copies the pixels out of `allocation` into our CPU buffer.
extern "C" fn on_present(
    user_data: *mut c_void,
    allocation: *const c_void,
    row_bytes: usize,
    height: usize,
) -> bool {
    let overlay = unsafe { &mut *(user_data as *mut FlutterOverlay) };
    let src = allocation as *const u8;
    let dst = overlay.pixel_buffer.as_mut_ptr();
    for y in 0..height {
        unsafe {
            std::ptr::copy_nonoverlapping(
                src.add(y * row_bytes),
                dst.add((y as usize) * (overlay.width as usize) * 4),
                (overlay.width as usize) * 4,
            );
        }
    }
    true
}

/// Off-screen Flutter overlay that renders via the **software** embedder into D3D11.
pub struct FlutterOverlay {
    /// Opaque Flutter engine handle.
    pub engine: FlutterEngineRef,
    /// CPU-side RGBA buffer (width×height×4 bytes).
    pub pixel_buffer: Vec<u8>,
    /// Dimensions of the Flutter viewport.
    pub width: u32,
    pub height: u32,
    /// D3D11 texture receiving Flutter’s pixels.
    pub texture: ID3D11Texture2D,
    /// Shader-resource view for sampling `texture`.
    pub srv: ID3D11ShaderResourceView,
}

impl FlutterOverlay {
    /// Initialize Flutter in **software**‐renderer mode, writing into a D3D11 texture.
    ///
    /// - `data_dir`: if `Some(path)` → `<path>/data/{flutter_assets,icudtl.dat,…}`, otherwise DLL’s dir.
    /// - `device`/`context`: your D3D11 device & immediate context.
    /// - `width`/`height`: pixel dimensions of the Flutter viewport.
    ///
    /// # Panics
    /// Panics if any D3D11 or Flutter API call fails.
    pub fn init(
        data_dir: Option<PathBuf>,
        device: &ID3D11Device,
        _context: &ID3D11DeviceContext,
        width: u32,
        height: u32,
    ) -> Self {
        // 1) Locate Flutter assets (UTF-16 on Windows).
        let (assets_wide, icu_wide, _aot_wide) = match data_dir.as_ref() {
            Some(dir) => path_utils::get_flutter_paths_from(dir),
            None => path_utils::get_flutter_paths(),
        };
        // Vec<u16> → OsString → Rust String → CString
        let assets_c = {
            let os: OsString = OsString::from_wide(&assets_wide);
            CString::new(os.to_string_lossy().into_owned()).unwrap()
        };
        let icu_c = {
            let os: OsString = OsString::from_wide(&icu_wide);
            CString::new(os.to_string_lossy().into_owned()).unwrap()
        };

        // 2) Create dynamic RGBA8 D3D11 texture & SRV.
        let tex_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let mut texture_opt = None;
        unsafe {
            device
                .CreateTexture2D(&tex_desc, None, Some(&mut texture_opt))
                .unwrap();
        }
        let texture = texture_opt.unwrap();

        let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: tex_desc.Format,
            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            Anonymous: unsafe { mem::zeroed() },
            ..Default::default()
        };
        let mut srv_opt = None;
        unsafe {
            device
                .CreateShaderResourceView(&texture, Some(&srv_desc), Some(&mut srv_opt))
                .unwrap();
        }
        let srv = srv_opt.unwrap();

        // 3) Build FlutterProjectArgs.
        let mut proj_args: FlutterProjectArgs = unsafe { mem::zeroed() };
        proj_args.struct_size = mem::size_of::<FlutterProjectArgs>();
        proj_args.assets_path = assets_c.as_ptr();
        proj_args.icu_data_path = icu_c.as_ptr();
        proj_args.dart_entrypoint_argc = 0;
        proj_args.dart_entrypoint_argv = std::ptr::null_mut();

        // 4) Build FlutterSoftwareRendererConfig.
        let mut sw_cfg: FlutterSoftwareRendererConfig = unsafe { mem::zeroed() };
        sw_cfg.struct_size = mem::size_of::<FlutterSoftwareRendererConfig>();
        sw_cfg.surface_present_callback = Some(on_present);

        // 5) Build FlutterRendererConfig.
        let mut rdr_cfg: FlutterRendererConfig = unsafe { mem::zeroed() };
        rdr_cfg.type_ = embedder::FlutterRendererType_kSoftware;

        rdr_cfg.__bindgen_anon_1.software = sw_cfg;

        // 6) Box our Rust state and grab a stable pointer for callbacks.
        let mut state = Box::new(FlutterOverlay {
            engine: std::ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture,
            srv,
        });
        let user_data = &mut *state as *mut _ as *mut c_void;

        // 7) Initialize & run the Flutter engine, passing our `user_data`.
        let mut engine: FlutterEngineRef = std::ptr::null_mut();
        let init_res = unsafe {
            embedder::FlutterEngineInitialize(
                FLUTTER_ENGINE_VERSION,
                &rdr_cfg,
                &proj_args,
                user_data, // ← here!
                &mut engine,
            )
        };
        assert_eq!(init_res, embedder::FlutterEngineResult_kSuccess);

        let run_res = unsafe { embedder::FlutterEngineRunInitialized(engine) };
        assert_eq!(run_res, embedder::FlutterEngineResult_kSuccess);

        state.engine = engine;
        *state
    }

    /// Pump Flutter (animations, timers) and upload the latest RGBA frame.
    /// Call **once per frame** before your D3D11 draw pass.
    pub fn tick(&mut self, context: &ID3D11DeviceContext) {
        unsafe {
            // 1) Run any pending Flutter tasks:
            embedder::FlutterEngineRunTask(self.engine, std::ptr::null());

            // 2) Map + memcpy into our D3D11 texture:
            let mut mapped: D3D11_MAPPED_SUBRESOURCE = mem::zeroed();
            context
                .Map(
                    &self.texture,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped),
                )
                .unwrap();

            let row_pitch = mapped.RowPitch as usize;
            let src_pitch = (self.width as usize) * 4;
            let src_ptr = self.pixel_buffer.as_ptr();

            for y in 0..(self.height as usize) {
                let dst = (mapped.pData as *mut u8).add(y * row_pitch);
                let src = src_ptr.add(y * src_pitch);
                std::ptr::copy_nonoverlapping(src, dst, src_pitch);
            }
            context.Unmap(&self.texture, 0);
        }
    }
}

// ─── Example Usage ─────────────────────────────────────────────────────────────
//
// ```no_run
// # use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext};
// # fn get_d3d() -> (ID3D11Device, ID3D11DeviceContext) { unimplemented!() }
// # let (device, context) = get_d3d();
// let mut overlay = FlutterOverlay::init(None, &device, &context, 800, 600);
// loop {
//     overlay.tick(&context);
//     // bind overlay.srv in your D3D11 pipeline, draw a quad, etc.
// }
// ```
