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

use crate::{
    embedder::{
        self, FlutterEngineAOTData, FlutterEngineAOTDataSource, FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath, FlutterEngineCreateAOTData, FlutterEngineInitialize, FlutterEngineResult_kSuccess, FlutterEngineRunInitialized, FlutterEngineRunTask, FlutterEngineSendWindowMetricsEvent, FlutterProjectArgs, FlutterRendererConfig, FlutterRendererType_kSoftware, FlutterSoftwareRendererConfig, FlutterWindowMetricsEvent
    },
    path_utils::{get_flutter_paths, get_flutter_paths_from},
};

/// Must match C `#define FLUTTER_ENGINE_VERSION 1`.
const FLUTTER_ENGINE_VERSION: usize = 1;

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
    pub fn new(
        data_dir: Option<PathBuf>,
        device: &ID3D11Device,
        width: u32,
        height: u32,
    ) -> Self {
        let overlay = FlutterOverlay::init(data_dir, device, width, height);
        EmbedderContext { overlay }
    }
}

impl FlutterOverlay {
    fn init(data_dir: Option<PathBuf>, device: &ID3D11Device, width: u32, height: u32) -> Self {
        println!("[init] Starting FlutterOverlay::init");
    
        // 1) Locate asset paths
        let (mut assets_wide, mut icu_wide, mut aot_wide) = match data_dir {
            Some(ref dir) => {
                println!("[init] Using custom data_dir: {:?}", dir);
                get_flutter_paths_from(dir)
            },
            None => {
                println!("[init] Using default flutter paths");
                get_flutter_paths()
            },
        };
        if assets_wide.last() == Some(&0) {
            assets_wide.pop();
        }
        if icu_wide.last() == Some(&0) {
            icu_wide.pop();
        }
        if aot_wide.last() == Some(&0) {
            aot_wide.pop();
        }
    
        // 2) Convert to UTF-8 CStrings:
        let assets_c = {
            let s = OsString::from_wide(&assets_wide)
                .to_string_lossy()
                .into_owned();
            println!("[init] assets_path = {}", s);
            CString::new(s).unwrap()
        };
        let icu_c = {
            let s = OsString::from_wide(&icu_wide)
                .to_string_lossy()
                .into_owned();
            println!("[init] icu_data_path = {}", s);
            CString::new(s).unwrap()
        };
    
        // 3) Create D3D11 texture:
        println!("[init] Creating D3D11 texture...");
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
        let texture = unsafe {
            let mut opt = None;
            device.CreateTexture2D(&tex_desc, None, Some(&mut opt)).expect("CreateTexture2D failed");
            println!("[init] Texture created successfully.");
            opt.unwrap()
        };
    
        // 4) Create SRV:
        println!("[init] Creating ShaderResourceView...");
        let srv = unsafe {
            let mut desc: D3D11_SHADER_RESOURCE_VIEW_DESC = mem::zeroed();
            desc.Format = tex_desc.Format;
            desc.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
            let mut opt = None;
            device.CreateShaderResourceView(&texture, None, Some(&mut opt)).expect("CreateShaderResourceView failed");
            println!("[init] ShaderResourceView created.");
            opt.unwrap()
        };
    
         // 5) Load AOT data if present and prepare aot_data_handle
         let mut aot_data_handle: FlutterEngineAOTData = std::ptr::null_mut();
         let _aot_c_string_holder: Option<CString> = if !aot_wide.is_empty() {
             println!("[init] AOT path provided, loading...");
             let aot_c = CString::new(OsString::from_wide(&aot_wide).to_string_lossy().as_ref()).unwrap();
             println!("[init] aot_path = {:?}", aot_c);
 
             let mut src: FlutterEngineAOTDataSource = unsafe { mem::zeroed() };
             src.type_ = FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath;
             src.__bindgen_anon_1.elf_path = aot_c.as_ptr();
 
             let r = unsafe { FlutterEngineCreateAOTData(&src, &mut aot_data_handle) };
             assert_eq!(r, FlutterEngineResult_kSuccess, "CreateAOTData failed: {:?}", r);
             println!("[init] AOT data loaded successfully.");
             Some(aot_c) // Keep CString alive
         } else {
             println!("[init] No AOT data found.");
             None
         };
 
         // 6) Build FlutterProjectArgs
         println!("[init] Building FlutterProjectArgs...");
         let mut proj_args: FlutterProjectArgs = unsafe { mem::zeroed() };
         proj_args.struct_size = std::mem::size_of::<FlutterProjectArgs>(); 
         proj_args.assets_path = assets_c.as_ptr();
         proj_args.icu_data_path = icu_c.as_ptr();
         proj_args.aot_data = aot_data_handle;
         // proj_args.command_line_argc = 0;
         // proj_args.command_line_argv = std::ptr::null();
 
 
         // 7) Renderer config
         println!("[init] Setting up renderer config...");
         let mut sw_cfg: FlutterSoftwareRendererConfig = unsafe { mem::zeroed() };
         sw_cfg.struct_size = std::mem::size_of::<FlutterSoftwareRendererConfig>();
         sw_cfg.surface_present_callback = Some(on_present);
 
         let mut rdr_cfg: FlutterRendererConfig = unsafe { mem::zeroed() };
         rdr_cfg.type_ = FlutterRendererType_kSoftware;
         rdr_cfg.__bindgen_anon_1.software = sw_cfg;
 
         // 8) Box and user_data
         println!("[init] Creating FlutterOverlay structure...");
         let mut overlay = Box::new(FlutterOverlay {
             engine: std::ptr::null_mut(),
             pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
             width,
             height,
             texture: texture,
             srv: srv,
         });
         let user_data = &mut *overlay as *mut _ as *mut c_void;
    
        // 9) Initialize engine
        println!("[init] Initializing Flutter engine...");
        let mut engine = std::ptr::null_mut();
        let init_r = unsafe {
            FlutterEngineInitialize(
                FLUTTER_ENGINE_VERSION,
                &rdr_cfg,
                &proj_args,
                user_data,
                &mut engine,
            )
        };
        assert_eq!(init_r, FlutterEngineResult_kSuccess, "Initialize failed: {:?}", init_r);
        println!("[init] Flutter engine initialized.");
    
        let run_r = unsafe { FlutterEngineRunInitialized(engine) };
        assert_eq!(run_r, FlutterEngineResult_kSuccess, "RunInitialized failed: {:?}", run_r);
        println!("[init] Flutter engine run initialized.");
    
        // 10) Send initial window metrics
        println!("[init] Sending initial window metrics...");
        let mut wm: FlutterWindowMetricsEvent = unsafe { std::mem::zeroed() };
        wm.struct_size = std::mem::size_of::<FlutterWindowMetricsEvent>();
        wm.width = width as usize;
        wm.height = height as usize;
        wm.pixel_ratio = 1.0;
        wm.left = 0;
        wm.top = 0;
        wm.physical_view_inset_top = 0.0;
        wm.physical_view_inset_right = 0.0;
        wm.physical_view_inset_bottom = 0.0;
        wm.physical_view_inset_left = 0.0;
        wm.display_id = 0;
        wm.view_id = 0;
        let res = unsafe { FlutterEngineSendWindowMetricsEvent(engine, &wm) };
        assert_eq!(res, FlutterEngineResult_kSuccess, "SendWindowMetricsEvent failed: {:?}", res);
        println!("[init] Window metrics sent.");
    
        // 11) Return overlay
        println!("[init] Initialization complete. Returning FlutterOverlay.");
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
