use std::{ ffi::{ c_char, c_void, CStr, CString, OsString }, mem, os::windows::ffi::OsStringExt, path::PathBuf, ptr::{ self } };
use windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE,
    D3D11_CPU_ACCESS_WRITE,
    D3D11_MAP_WRITE_DISCARD,
    D3D11_MAPPED_SUBRESOURCE,
    D3D11_SHADER_RESOURCE_VIEW_DESC,
    D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DYNAMIC,
    ID3D11Device,
    ID3D11DeviceContext,
    ID3D11ShaderResourceView,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{ DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC };

use crate::{embedder::{ self, FlutterEngineRunTask }, path_utils::{get_flutter_paths, get_flutter_paths_from}};

const FLUTTER_ENGINE_VERSION: usize = 1;

unsafe extern "C" fn flutter_log_callback(
    tag: *const c_char,
    message: *const c_char,
    _user_data: *mut c_void
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
            "[init] Starting FlutterOverlay::init with width={}, height={}",
            width, height
        );
        if width == 0 || height == 0 {
            panic!("[init] ERROR: Received width ({}) or height ({}) is zero!", width, height);
        }

        // 1) Pfade ermitteln: entweder aus data_dir oder aus DLL-Verzeichnis
        let (assets_w, icu_w, aot_w) = match data_dir.as_ref() {
            Some(dir) => get_flutter_paths_from(dir),
            None      => get_flutter_paths(),
        };

        // 2) Wide-Strings in OsString umwandeln (Null-Terminierer entfernen)
        let strip_nul = |mut v: Vec<u16>| { if v.last() == Some(&0) { v.pop(); } v };
        let assets_os = OsString::from_wide(&strip_nul(assets_w));
        let icu_os    = OsString::from_wide(&strip_nul(icu_w));
        let aot_vec   = strip_nul(aot_w);

        println!("[init] assets_path: {:?}", assets_os);
        println!("[init] icu_data_path: {:?}", icu_os);
        println!("[init] aot_path (raw u16s): len={}", aot_vec.len());

        // 3) In CString für embedder-API
        let assets_c = CString::new(assets_os.to_string_lossy().as_bytes())
            .expect("CString für assets_path fehlgeschlagen");
        let icu_c = CString::new(icu_os.to_string_lossy().as_bytes())
            .expect("CString für icu_data_path fehlgeschlagen");

        // 4) D3D11-Textur anlegen
        println!("[init] Creating D3D11 texture ({}x{})...", width, height);
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
                .expect("CreateTexture2D fehlgeschlagen");
            tx.unwrap()
        };
        println!("[init] Texture erstellt.");

        // 5) ShaderResourceView anlegen
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
                .expect("CreateShaderResourceView fehlgeschlagen");
            view.unwrap()
        };
        println!("[init] SRV erstellt.");

        // 6) Overlay-Struct auf dem Heap speichern
        println!("[init] Allocating FlutterOverlay struct on heap...");
        let boxed = Box::new(FlutterOverlay {
            engine: ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture,
            srv,
        });
        let raw_ptr = Box::into_raw(boxed);
        unsafe {
            if !FLUTTER_OVERLAY_RAW_PTR.is_null() {
                panic!("[init] ERROR: FLUTTER_OVERLAY_RAW_PTR bereits gesetzt!");
            }
            FLUTTER_OVERLAY_RAW_PTR = raw_ptr;
        }
        let user_data = raw_ptr as *mut c_void;

        // 7) FlutterProjectArgs befüllen
        let mut proj_args: embedder::FlutterProjectArgs = unsafe { mem::zeroed() };
        proj_args.struct_size   = mem::size_of::<embedder::FlutterProjectArgs>();
        proj_args.assets_path   = assets_c.as_ptr();
        proj_args.icu_data_path = icu_c.as_ptr();

        // 8) Optional AOT oder JIT (leerer aot_vec → JIT/Kernal-Modus)
        let mut aot_c_holder: Option<CString> = None;
        if aot_vec.is_empty() {
            proj_args.aot_data = ptr::null_mut();
            println!("[init] Kein AOT-Pfad, wechsle in JIT/Kernal-Modus.");
        } else {
            let aot_os = OsString::from_wide(&aot_vec);
            let aot_c  = CString::new(aot_os.to_string_lossy().as_bytes())
                .expect("CString für AOT-Pfad fehlgeschlagen");
            println!("[init] Lade AOT-Daten von {:?}", aot_os);
            let res = unsafe {
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
            if res != embedder::FlutterEngineResult_kSuccess {
                unsafe {
                    drop(Box::from_raw(raw_ptr));
                    FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut();
                }
                panic!("[init] FlutterEngineCreateAOTData fehlgeschlagen: {:?}", res);
            }
            println!("[init] AOT-Daten geladen.");
            aot_c_holder = Some(aot_c);
        }

        // 9) Kommandozeilen-Argumente & Renderer-Config
        let argv_store = vec![
            CString::new("dummy_app_name").unwrap(),
            CString::new("--verbose-system-logs").unwrap(),
            CString::new("--enable-vm-service").unwrap(),
        ];
        let argv_ptrs: Vec<*const c_char> = argv_store.iter().map(|s| s.as_ptr()).collect();
        proj_args.command_line_argc         = argv_ptrs.len() as i32;
        proj_args.command_line_argv         = argv_ptrs.as_ptr();
        proj_args.platform_message_callback = None;
        proj_args.log_message_callback      = Some(flutter_log_callback);
        proj_args.log_tag                   = FLUTTER_LOG_TAG.as_ptr();

        let mut sw_cfg: embedder::FlutterSoftwareRendererConfig = unsafe { mem::zeroed() };
        sw_cfg.struct_size                      = mem::size_of::<embedder::FlutterSoftwareRendererConfig>();
        sw_cfg.surface_present_callback         = Some(on_present);
        let mut rdr_cfg: embedder::FlutterRendererConfig = unsafe { mem::zeroed() };
        rdr_cfg.type_                            = embedder::FlutterRendererType_kSoftware;
        rdr_cfg.__bindgen_anon_1.software        = sw_cfg;

        // 10) Engine starten
        println!("[init] Calling FlutterEngineRun...");
        let mut engine_handle: embedder::FlutterEngine = ptr::null_mut();
        let run_res = unsafe {
            embedder::FlutterEngineRun(
                FLUTTER_ENGINE_VERSION,
                &rdr_cfg,
                &proj_args,
                user_data,
                &mut engine_handle,
            )
        };
        drop(aot_c_holder);
        if run_res != embedder::FlutterEngineResult_kSuccess {
            unsafe {
                drop(Box::from_raw(raw_ptr));
                FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut();
            }
            panic!("[init] FlutterEngineRun fehlgeschlagen: {:?}", run_res);
        }
        unsafe { (*raw_ptr).engine = engine_handle; }
        println!("[init] Engine gestartet.");

        // 11) Fenster-Metriken senden
        let mut wm: embedder::FlutterWindowMetricsEvent = unsafe { mem::zeroed() };
        wm.struct_size = mem::size_of::<embedder::FlutterWindowMetricsEvent>();
        wm.width       = width as usize;
        wm.height      = height as usize;
        wm.pixel_ratio = 1.0;
        let _ = unsafe { embedder::FlutterEngineSendWindowMetricsEvent(engine_handle, &wm) };

        // 12) Klonen und zurückgeben
        println!("[init] Complete. Cloning and returning.");
        unsafe {
            let tmp = Box::from_raw(raw_ptr);
            let result = (*tmp).clone();
            FLUTTER_OVERLAY_RAW_PTR = Box::into_raw(tmp);
            result
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
            match context.Map(&overlay.texture, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut m)) {
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
                            texture_row_pitch,
                            buffer_row_pitch
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
                    for y in 0..overlay.height as usize {
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
    height_flutter: usize
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
            ov.width,
            ov.height
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
