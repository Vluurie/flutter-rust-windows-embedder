use gag::BufferRedirect;
use std::{
    ffi::{CStr, CString, OsString, c_char, c_void},
    io::Read,
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

use crate::{
    embedder::{
        self, FlutterEngineAOTData, FlutterEngineAOTDataSource,
        FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
        FlutterEngineCreateAOTData, FlutterEngineInitialize, FlutterEngineResult_kSuccess,
        FlutterEngineRunInitialized, FlutterEngineRunTask, FlutterEngineSendWindowMetricsEvent,
        FlutterProjectArgs, FlutterRendererConfig, FlutterRendererType_kSoftware,
        FlutterSoftwareRendererConfig, FlutterWindowMetricsEvent,
    },
    path_utils::{get_flutter_paths, get_flutter_paths_from},
};

/// Must match C `#define FLUTTER_ENGINE_VERSION 1`.
const FLUTTER_ENGINE_VERSION: usize = 1;

unsafe extern "C" fn flutter_log_callback(
    tag: *const c_char,
    message: *const c_char,
    _user_data: *mut c_void,
) {
    unsafe {
        let tag = CStr::from_ptr(tag).to_string_lossy();
        let msg = CStr::from_ptr(message).to_string_lossy();
        println!("[Flutter][{}] {}", tag, msg);
    }
}

static FLUTTER_LOG_TAG: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"rust_embedder\0") };

pub struct EmbedderContext {
    pub overlay: FlutterOverlay,
}

pub struct FlutterOverlay {
    pub engine: embedder::FlutterEngine,
    pub pixel_buffer: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub texture: ID3D11Texture2D,
    pub srv: ID3D11ShaderResourceView,
}

impl EmbedderContext {
    /// Initialize the software‐renderer embedder.
    pub fn new(data_dir: Option<PathBuf>, device: &ID3D11Device, width: u32, height: u32) -> Self {
        let overlay = FlutterOverlay::init(data_dir, device, width, height);
        EmbedderContext { overlay }
    }
}

impl FlutterOverlay {
    /// Initialize a new `FlutterOverlay`:
    ///
    /// 1. Find asset/ICU/AOT paths (from `data_dir` or defaults).  
    /// 2. Convert those wide-char paths to `CString`.  
    /// 3. Create a dynamic D3D11 texture + SRV of size `width`×`height`.  
    /// 4. If an AOT ELF is present, canonicalize the path, verify it exists,
    ///    then call `FlutterEngineCreateAOTData` under stderr capture, panicking
    ///    with the full error text on failure.  
    /// 5. Build `FlutterProjectArgs` pointing at assets, ICU, and (optional) AOT,
    ///    and pass `--verbose-system-logs` so all embedder errors go to stderr.  
    /// 6. Configure software renderer callback.  
    /// 7. Box the `FlutterOverlay` and pass it as `user_data`.  
    /// 8. Call `FlutterEngineInitialize`, `FlutterEngineRunInitialized`, and
    ///    `FlutterEngineSendWindowMetricsEvent` each under stderr capture,
    ///    panicking with full text on any non-`kSuccess`.  
    ///
    /// # Panics
    /// Panics if any embedder call returns a non-`kSuccess` enum, printing
    /// the exact stderr output from the engine.
    pub fn init(
        data_dir: Option<PathBuf>,
        device: &ID3D11Device,
        width: u32,
        height: u32,
    ) -> Self {
        println!("[init] Starting FlutterOverlay::init");

        // 1) Locate paths
        let (mut assets_wide, mut icu_wide, mut aot_wide) = match data_dir {
            Some(ref dir) => {
                println!("[init] Using custom data_dir: {:?}", dir);
                get_flutter_paths_from(dir)
            }
            None => {
                println!("[init] Using default flutter paths");
                get_flutter_paths()
            }
        };
        for w in [&mut assets_wide, &mut icu_wide, &mut aot_wide] {
            if w.last() == Some(&0) { w.pop(); }
        }

        // 2) Wide→CString
        let assets_c = {
            let s = OsString::from_wide(&assets_wide).to_string_lossy().into_owned();
            println!("[init] assets_path = {}", s);
            CString::new(s).unwrap()
        };
        let icu_c = {
            let s = OsString::from_wide(&icu_wide).to_string_lossy().into_owned();
            println!("[init] icu_data_path = {}", s);
            CString::new(s).unwrap()
        };

        // 3) Create D3D11 texture
        println!("[init] Creating D3D11 texture...");
        let tex_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let texture = unsafe {
            let mut tx = None;
            device
                .CreateTexture2D(&tex_desc, None, Some(&mut tx))
                .expect("CreateTexture2D failed");
            println!("[init] Texture created successfully.");
            tx.unwrap()
        };

        // 4) Create SRV
        println!("[init] Creating ShaderResourceView...");
        let srv = unsafe {
            let mut desc: D3D11_SHADER_RESOURCE_VIEW_DESC = mem::zeroed();
            desc.Format = tex_desc.Format;
            desc.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
            let mut view = None;
            device
                .CreateShaderResourceView(&texture, None, Some(&mut view))
                .expect("CreateShaderResourceView failed");
            println!("[init] ShaderResourceView created.");
            view.unwrap()
        };

        // 5) Build FlutterProjectArgs (with verbose-system-logs)
        println!("[init] Building FlutterProjectArgs...");

        let argv_store = {
            let flag = CString::new("--verbose-system-logs").unwrap();
            vec![flag]
        };
        let argv_ptrs: Vec<*const c_char> = argv_store.iter().map(|s| s.as_ptr()).collect();
    
        println!("[init] Building FlutterProjectArgs...");
        let mut proj_args: FlutterProjectArgs = unsafe { mem::zeroed() };
    
        proj_args.struct_size = mem::size_of::<FlutterProjectArgs>();
    
        proj_args.assets_path   = assets_c.as_ptr();
        proj_args.icu_data_path = icu_c.as_ptr();
    
        proj_args.aot_data = std::ptr::null_mut();
    
        proj_args.command_line_argc = argv_ptrs.len() as i32;
        proj_args.command_line_argv = argv_ptrs.as_ptr();
    
        proj_args.platform_message_callback = None;
        proj_args.log_message_callback = Some(flutter_log_callback);
        proj_args.log_tag              = FLUTTER_LOG_TAG.as_ptr();

        // 6) Load AOT data if provided
        println!("[init] Loading AOT data if available...");
        if !aot_wide.is_empty() {
            let raw = OsString::from_wide(&aot_wide).to_string_lossy().into_owned();
            let canon = PathBuf::from(&raw)
                .canonicalize()
                .unwrap_or_else(|e| panic!("canonicalize {:?} failed: {}", raw, e));
            println!("[init] canonical aot_path = {:?}", canon);
            assert!(canon.exists(), "AOT ELF not found at {:?}", canon);

            let aot_c = CString::new(canon.to_string_lossy().as_ref()).unwrap();
            let mut buf = BufferRedirect::stderr().unwrap();
            let result = unsafe {
                FlutterEngineCreateAOTData(
                    &FlutterEngineAOTDataSource {
                        type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
                        __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                            elf_path: aot_c.as_ptr(),
                        },
                    },
                    &mut proj_args.aot_data,
                )
            };
            if result != FlutterEngineResult_kSuccess {
                let mut err = String::new();
                buf.read_to_string(&mut err).unwrap();
                panic!("FlutterEngineCreateAOTData failed ({:?}): {}", result, err);
            }
            println!("[init] AOT data loaded successfully.");
        } else {
            println!("[init] No AOT data found; continuing.");
        }

        // 7) Configure software renderer
        println!("[init] Configuring software renderer...");
        let mut sw_cfg: FlutterSoftwareRendererConfig = unsafe { mem::zeroed() };
        sw_cfg.surface_present_callback = Some(on_present);
        let mut rdr_cfg: FlutterRendererConfig = unsafe { mem::zeroed() };
        rdr_cfg.type_ = FlutterRendererType_kSoftware;
        rdr_cfg.__bindgen_anon_1.software = sw_cfg;

        // 8) Box overlay + user_data
        println!("[init] Allocating FlutterOverlay struct...");
        let mut overlay = Box::new(FlutterOverlay {
            engine: std::ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture,
            srv,
        });
        let user_data = &mut *overlay as *mut _ as *mut _;

        // 9) FlutterEngineInitialize
        println!("[init] Calling FlutterEngineInitialize...");
        let mut engine = std::ptr::null_mut();
        let mut buf = BufferRedirect::stderr().unwrap();
        let init_r = unsafe {
            FlutterEngineInitialize(
                FLUTTER_ENGINE_VERSION,
                &rdr_cfg,
                &proj_args,
                user_data,
                &mut engine,
            )
        };
        if init_r != FlutterEngineResult_kSuccess {
            let mut err = String::new();
            buf.read_to_string(&mut err).unwrap();
            panic!("FlutterEngineInitialize failed ({:?}): {}", init_r, err);
        }
        println!("[init] Flutter engine initialized.");

        // 10) FlutterEngineRunInitialized
        println!("[init] Running FlutterEngineRunInitialized...");
        let mut buf = BufferRedirect::stderr().unwrap();
        let run_r = unsafe { FlutterEngineRunInitialized(engine) };
        if run_r != FlutterEngineResult_kSuccess {
            let mut err = String::new();
            buf.read_to_string(&mut err).unwrap();
            panic!("FlutterEngineRunInitialized failed ({:?}): {}", run_r, err);
        }
        println!("[init] Flutter engine run initialized.");

        // 11) Send window metrics
        println!("[init] Sending initial window metrics...");
        let mut wm: FlutterWindowMetricsEvent = unsafe { mem::zeroed() };
        wm.width                     = width as usize;
        wm.height                    = height as usize;
        wm.pixel_ratio               = 1.0;
        wm.left                      = 0;
        wm.top                       = 0;
        wm.physical_view_inset_top   = 0.0;
        wm.physical_view_inset_right = 0.0;
        wm.physical_view_inset_bottom= 0.0;
        wm.physical_view_inset_left  = 0.0;
        wm.display_id                = 0;
        wm.view_id                   = 0;

        let mut buf = BufferRedirect::stderr().unwrap();
        let metrics_r = unsafe { FlutterEngineSendWindowMetricsEvent(engine, &wm) };
        if metrics_r != FlutterEngineResult_kSuccess {
            let mut err = String::new();
            buf.read_to_string(&mut err).unwrap();
            panic!(
                "FlutterEngineSendWindowMetricsEvent failed ({:?}): {}",
                metrics_r, err
            );
        }
        println!("[init] Window metrics sent.");

        // 12) Finish
        println!("[init] Initialization complete.");
        overlay.engine = engine;
        *overlay
    }

    /// Pump tasks and upload the latest frame to the D3D11 texture.
    pub fn tick(&mut self, context: &ID3D11DeviceContext) {
        unsafe {
            FlutterEngineRunTask(self.engine, std::ptr::null());
            let mut m: D3D11_MAPPED_SUBRESOURCE = mem::zeroed();
            context
                .Map(&self.texture, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut m))
                .unwrap();
            let row_pitch = m.RowPitch as usize;
            let src_pitch = (self.width as usize) * 4;
            let src_ptr = self.pixel_buffer.as_ptr();
            for y in 0..(self.height as usize) {
                let dst = (m.pData as *mut u8).add(y * row_pitch);
                let src = src_ptr.add(y * src_pitch);
                std::ptr::copy_nonoverlapping(src, dst, src_pitch);
            }
            context.Unmap(&self.texture, 0);
        }
    }
}

extern "C" fn on_present(
    user_data: *mut c_void,
    allocation: *const c_void,
    row_bytes: usize,
    height: usize,
) -> bool {
    let ov = unsafe { &mut *(user_data as *mut FlutterOverlay) };
    let src = allocation as *const u8;
    let dst = ov.pixel_buffer.as_mut_ptr();
    for y in 0..height {
        unsafe {
            std::ptr::copy_nonoverlapping(
                src.add(y * row_bytes),
                dst.add((y as usize) * (ov.width as usize) * 4),
                (ov.width as usize) * 4,
            );
        }
    }
    true
}
