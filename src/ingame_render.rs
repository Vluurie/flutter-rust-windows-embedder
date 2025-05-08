use gag::BufferRedirect;
use std::{
    ffi::{c_char, c_void, CStr, CString, OsString},
    io::Read,
    mem,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    ptr::{self, null_mut},
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
        self, FlutterEngineAOTDataSource, FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath, FlutterEngineAOTDataSource__bindgen_ty_1, FlutterEngineCreateAOTData, FlutterEngineResult_kSuccess, FlutterEngineRun, FlutterEngineRunTask, FlutterEngineSendWindowMetricsEvent, FlutterProjectArgs, FlutterRendererConfig, FlutterRendererType_kSoftware, FlutterSoftwareRendererConfig, FlutterWindowMetricsEvent
    },
    path_utils::{get_flutter_paths, get_flutter_paths_from},
};

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
pub static mut FLUTTER_OVERLAY_RAW_PTR: *mut FlutterOverlay = ptr::null_mut();
static FLUTTER_LOG_TAG: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"rust_embedder\0") };

pub struct EmbedderContext {
    pub overlay: FlutterOverlay,
}

#[derive(Clone)]
#[repr(C)]
pub struct FlutterOverlay {
    pub engine: embedder::FlutterEngine,
    pub pixel_buffer: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub texture: ID3D11Texture2D,
    pub srv: ID3D11ShaderResourceView,
}

impl EmbedderContext {
    pub fn new(data_dir: Option<PathBuf>, device: &ID3D11Device, width: u32, height: u32) -> Self {
        let overlay = FlutterOverlay::init(data_dir, device, width, height);
        EmbedderContext { overlay }
    }
}

impl FlutterOverlay {
    pub fn init(data_dir: Option<PathBuf>, device: &ID3D11Device, width: u32, height: u32) -> Self {
        println!(
            "[init] Starting FlutterOverlay::init with received width={}, height={}",
            width, height
        );
    
        if width == 0 || height == 0 {
            panic!(
                "[init] ERROR: Received width ({}) or height ({}) is zero!",
                width, height
            );
        }
    
        
        
        
        const HARDCODED_ASSETS_PATH: &str = "F:\\test_zwo\\build\\windows\\x64\\runner\\Release\\data\\flutter_assets";
        const HARDCODED_ICU_PATH: &str = "F:\\test_zwo\\build\\windows\\x64\\runner\\Release\\data\\icudtl.dat";
        
        const HARDCODED_AOT_PATH_OPT: Option<&str> = Some("F:\\test_zwo\\build\\windows\\x64\\runner\\Release\\data\\app.so");
        
    
        
        let (assets_c, icu_c, aot_path_str_opt) = {
            println!("[init] Using hardcoded assets_path: {}", HARDCODED_ASSETS_PATH);
            println!("[init] Using hardcoded icu_data_path: {}", HARDCODED_ICU_PATH);
            if let Some(p) = HARDCODED_AOT_PATH_OPT {
                 println!("[init] Using hardcoded AOT path: {}", p);
            } else {
                 println!("[init] Using hardcoded paths: No AOT path specified.");
            }
    
            
            if !PathBuf::from(HARDCODED_ASSETS_PATH).exists() {
                 panic!("[init] Hardcoded assets path does not exist: {}", HARDCODED_ASSETS_PATH);
            }
             if !PathBuf::from(HARDCODED_ICU_PATH).exists() {
                 panic!("[init] Hardcoded icu_data path does not exist: {}", HARDCODED_ICU_PATH);
            }
            if let Some(p) = HARDCODED_AOT_PATH_OPT {
                 if !PathBuf::from(p).exists() {
                     panic!("[init] Hardcoded AOT path does not exist: {}", p);
                 }
            }
    
    
            (
                CString::new(HARDCODED_ASSETS_PATH)
                    .expect("Failed to create CString for hardcoded assets_path"),
                CString::new(HARDCODED_ICU_PATH)
                    .expect("Failed to create CString for hardcoded icu_data_path"),
                HARDCODED_AOT_PATH_OPT, 
            )
        };
        
    
    
        
        
        println!("[init] Creating D3D11 texture ({}x{})...", width, height);
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
            let mut tx = None;
            device
                .CreateTexture2D(&tex_desc, None, Some(&mut tx))
                .map_err(|e| format!("CreateTexture2D failed: HRESULT {}", e.code().0))
                .expect("CreateTexture2D failure");
            tx.unwrap()
        };
        println!("[init] Texture created successfully.");
    
        println!("[init] Creating ShaderResourceView...");
        let srv = unsafe {
            let mut desc: D3D11_SHADER_RESOURCE_VIEW_DESC = mem::zeroed();
            desc.Format = tex_desc.Format;
            desc.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
            desc.Anonymous.Texture2D.MipLevels = tex_desc.MipLevels;
            desc.Anonymous.Texture2D.MostDetailedMip = 0;
            let mut view = None;
            device
                .CreateShaderResourceView(&texture, None, Some(&mut view))
                .map_err(|e| format!("CreateShaderResourceView failed: HRESULT {}", e.code().0))
                .expect("CreateShaderResourceView failure");
            view.unwrap()
        };
        println!("[init] ShaderResourceView created.");
    
        
        
        println!("[init] Allocating FlutterOverlay struct on heap...");
        let overlay_boxed = Box::new(FlutterOverlay {
            engine: ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture,
            srv,
        });
        let overlay_raw_ptr = Box::into_raw(overlay_boxed);
        unsafe {
            if !FLUTTER_OVERLAY_RAW_PTR.is_null() {
                panic!("[init] ERROR: FLUTTER_OVERLAY_RAW_PTR is already set!");
            }
            FLUTTER_OVERLAY_RAW_PTR = overlay_raw_ptr;
            println!(
                "[init] Stored raw pointer globally: {:?}",
                FLUTTER_OVERLAY_RAW_PTR
            );
        }
        let user_data = overlay_raw_ptr as *mut c_void;
    
        
        
        let mut proj_args: embedder::FlutterProjectArgs = unsafe { mem::zeroed() };
        proj_args.struct_size = mem::size_of::<embedder::FlutterProjectArgs>();
        proj_args.assets_path = assets_c.as_ptr();
        proj_args.icu_data_path = icu_c.as_ptr(); 
    
        
        println!("[init] Setting up AOT data if path was found...");
        let mut aot_c_holder: Option<CString> = None; 
    
        if let Some(aot_path_str) = aot_path_str_opt {
            println!("[init] AOT path found (hardcoded), processing: {}", aot_path_str);
            
            
    
            let aot_c = CString::new(aot_path_str)
                 .expect("Failed to create CString for hardcoded AOT path");
            {
                
                
                println!("[init] Calling FlutterEngineCreateAOTData...");
                let result = unsafe {
                    embedder::FlutterEngineCreateAOTData(
                        &embedder::FlutterEngineAOTDataSource {
                            type_: embedder::FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
                            __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                                elf_path: aot_c.as_ptr(), 
                            },
                        },
                        &mut proj_args.aot_data,
                    )
                };
                println!("[DEBUG] FlutterEngineCreateAOTData result code: {:?}", result);
                if result != embedder::FlutterEngineResult_kSuccess {
                    
                    
                    eprintln!(
                        "[init] FlutterEngineCreateAOTData failed ({:?})", 
                        result 
                    );
                    unsafe {
                        drop(Box::from_raw(overlay_raw_ptr));
                        FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut();
                    }
                    panic!("FlutterEngineCreateAOTData failed! Result: {:?}", result);
                }
            }
            println!("[init] AOT data loaded successfully and linked to proj_args.");
            aot_c_holder = Some(aot_c); 
        } else {
            println!("[init] No AOT data path found; continuing in JIT/Kernel mode.");
            proj_args.aot_data = ptr::null_mut();
        }
    
        
        
        let argv_store: Vec<CString> = vec![ CString::new("dummy_app_name").unwrap(),CString::new("--verbose-system-logs").unwrap(), CString::new("--enable-vm-service").unwrap()];
        let argv_ptrs: Vec<*const c_char> = argv_store.iter().map(|s| s.as_ptr()).collect();
        proj_args.command_line_argc = argv_ptrs.len() as i32;
        proj_args.command_line_argv = argv_ptrs.as_ptr();
        proj_args.platform_message_callback = None;
        proj_args.log_message_callback = Some(flutter_log_callback);
        proj_args.log_tag = FLUTTER_LOG_TAG.as_ptr();
    
        
        
        println!("[init] Configuring software renderer...");
        let mut sw_cfg: embedder::FlutterSoftwareRendererConfig = unsafe { mem::zeroed() };
        sw_cfg.struct_size = mem::size_of::<embedder::FlutterSoftwareRendererConfig>();
        sw_cfg.surface_present_callback = Some(on_present);
    
        let mut rdr_cfg: embedder::FlutterRendererConfig = unsafe { mem::zeroed() };
        rdr_cfg.type_ = embedder::FlutterRendererType_kSoftware;
        rdr_cfg.__bindgen_anon_1.software = sw_cfg;
    
        
        
        println!("[init] Calling FlutterEngineRun...");
        let mut engine_handle: embedder::FlutterEngine = ptr::null_mut();
        let run_result = unsafe {
            embedder::FlutterEngineRun(
                FLUTTER_ENGINE_VERSION, 
                &rdr_cfg,
                &proj_args,
                user_data,
                &mut engine_handle,
            )
        };
    
        
        
        drop(aot_c_holder);
    
    
        if run_result != embedder::FlutterEngineResult_kSuccess {
            eprintln!("[init] FlutterEngineRun failed ({:?})", run_result);
            unsafe {
                drop(Box::from_raw(overlay_raw_ptr));
                FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut();
            }
            panic!("FlutterEngineRun failed with {:?}", run_result);
        }
        println!("[init] FlutterEngineRun succeeded.");
        unsafe {
            (*overlay_raw_ptr).engine = engine_handle;
        }
    
        
        
        println!("[init] Sending initial window metrics...");
        let mut wm: embedder::FlutterWindowMetricsEvent = unsafe { mem::zeroed() };
        wm.struct_size = mem::size_of::<embedder::FlutterWindowMetricsEvent>();
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
        let metrics_r = unsafe { embedder::FlutterEngineSendWindowMetricsEvent(engine_handle, &wm) };
        if metrics_r != embedder::FlutterEngineResult_kSuccess {
            eprintln!(
                "[init] FlutterEngineSendWindowMetricsEvent failed ({:?})",
                metrics_r
            );
        }
    
        
        
        println!("[init] Initialization fully complete. Cloning data and returning Self.");
        unsafe {
            let temp_box_for_clone = Box::from_raw(overlay_raw_ptr);
            let owned_return_value = (*temp_box_for_clone).clone();
            FLUTTER_OVERLAY_RAW_PTR = Box::into_raw(temp_box_for_clone);
            owned_return_value
        }
    }

    pub unsafe fn tick_global(context: &ID3D11DeviceContext) {
        unsafe {
            let overlay_ptr = FLUTTER_OVERLAY_RAW_PTR;

            if overlay_ptr.is_null() || (*overlay_ptr).engine.is_null() {
                return;
            }

            let overlay = &mut *overlay_ptr;

            FlutterEngineRunTask(overlay.engine, ptr::null());

            if overlay.width == 0 || overlay.height == 0 {
                return;
            }

            let mut m: D3D11_MAPPED_SUBRESOURCE = mem::zeroed();
            match context.Map(
                &overlay.texture,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut m),
            ) {
                Ok(_) => {
                    if m.pData.is_null() {
                        eprintln!(
                            "[tick_global] ERROR: Mapped pData is null after successful Map call."
                        );
                        context.Unmap(&overlay.texture, 0);
                        return;
                    }
                    let texture_row_pitch = m.RowPitch as usize;
                    let buffer_row_pitch = (overlay.width as usize) * 4;

                    if texture_row_pitch < buffer_row_pitch {
                        eprintln!(
                            "[tick_global] ERROR: Texture RowPitch ({}) is less than buffer pitch ({}). Cannot copy.",
                            texture_row_pitch, buffer_row_pitch
                        );
                        context.Unmap(&overlay.texture, 0);
                        return;
                    }
                    if overlay.pixel_buffer.len() < buffer_row_pitch * (overlay.height as usize) {
                        eprintln!(
                            "[tick_global] ERROR: pixel_buffer is smaller than expected ({} bytes required, {} available). Cannot copy safely.",
                            buffer_row_pitch * (overlay.height as usize),
                            overlay.pixel_buffer.len()
                        );
                        context.Unmap(&overlay.texture, 0);
                        return;
                    }

                    let src_base_ptr = overlay.pixel_buffer.as_ptr();
                    for y in 0..(overlay.height as usize) {
                        let dst_row_ptr = (m.pData as *mut u8).add(y * texture_row_pitch);
                        let src_row_ptr = src_base_ptr.add(y * buffer_row_pitch);
                        ptr::copy_nonoverlapping(src_row_ptr, dst_row_ptr, buffer_row_pitch);
                    }
                    context.Unmap(&overlay.texture, 0);
                }
                Err(e) => {
                    eprintln!("[tick_global] ERROR: Failed to map texture: {:?}", e);
                }
            }
        }
    }
}

extern "C" fn on_present(
    user_data: *mut c_void,
    allocation: *const c_void,
    row_bytes_flutter: usize,
    height_flutter: usize,
) -> bool {
    if user_data.is_null() {
        eprintln!("[on_present] ERROR: user_data is NULL!");
        return true;
    }
    let ov = unsafe { &mut *(user_data as *mut FlutterOverlay) };

    if allocation.is_null() {
        eprintln!("[on_present] ERROR: Flutter allocation pointer is NULL!");
        return true;
    }
    if ov.width == 0 || ov.height == 0 || ov.pixel_buffer.is_empty() {
        eprintln!(
            "[on_present] ERROR: Overlay dimensions zero or pixel_buffer empty (width={}, height={})!",
            ov.width, ov.height
        );
        return true;
    }

    let overlay_pitch = (ov.width as usize) * 4;
    let copy_height = std::cmp::min(height_flutter, ov.height as usize);
    let bytes_to_copy_per_row = std::cmp::min(row_bytes_flutter, overlay_pitch);

    if bytes_to_copy_per_row == 0 || copy_height == 0 {
        return true;
    }

    let src_base = allocation as *const u8;
    let dst_base = ov.pixel_buffer.as_mut_ptr();
    for y in 0..copy_height {
        unsafe {
            let src_ptr_row = src_base.add(y * row_bytes_flutter);
            let dst_ptr_row = dst_base.add(y * overlay_pitch);
            std::ptr::copy_nonoverlapping(src_ptr_row, dst_ptr_row, bytes_to_copy_per_row);
        }
    }
    true
}
