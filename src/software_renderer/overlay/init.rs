use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::engine::{run_engine, update_flutter_window_metrics};
use crate::software_renderer::overlay::overlay_impl::{FLUTTER_LOG_TAG, FLUTTER_OVERLAY_RAW_PTR};
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::platform_message_callback::{set_global_engine_for_platform_messages, simple_platform_message_callback};
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, flutter_log_callback, maybe_load_aot_path_to_cstring
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;

use crate::embedder::{self, FlutterCustomTaskRunners, FlutterEngineAOTDataSource, FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath, FlutterEngineResult_kSuccess, FlutterProjectArgs};
use crate::software_renderer::overlay::textinput::text_input_set_global_engine;

use log::{error, info};
use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{ffi::c_void, path::PathBuf, ptr};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use super::overlay_impl::FlutterOverlay;

const FLUTTER_ENGINE_VERSION: usize = 1;
pub(crate) static FLUTTER_ASSETS_IS_DEBUG: AtomicBool = AtomicBool::new(false);

pub(crate) fn init_overlay(
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    width: u32,
    height: u32,
    dart_args_opt: Option<&[String]>,
) -> Box<FlutterOverlay> {
    unsafe {
        let engine_dll_load_dir = data_dir.as_deref();
        let engine_dll_arc = FlutterEngineDll::get_for(engine_dll_load_dir).unwrap_or_else(|e| {
            error!(
                "Failed to load flutter_engine.dll from `{:?}`: {:?}",
                engine_dll_load_dir, e
            );
            std::process::exit(1);
        });
        info!(
            "Loaded flutter_engine.dll from `{}`",
            engine_dll_load_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "EXE folder".into())
        );

        assert!(width > 0 && height > 0, "Width and height must be non-zero");

        let (assets, icu, aot_opt) = load_flutter_paths(data_dir.clone());
        let texture = create_texture(device, width, height);
        let srv = create_srv(device, &texture);

        let (
            assets_c_temp,
            icu_c_temp,
            engine_argv_cs_temp,
            dart_argv_cs_temp,
            platform_context_owner_temp,
            platform_description_owner_temp,
            custom_runners_struct_owner_temp,
        ) = build_project_args_and_strings(
            &assets.to_string_lossy(),
            &icu.to_string_lossy(),
            dart_args_opt,
        );

        let aot_c_temp = maybe_load_aot_path_to_cstring(aot_opt.as_deref());

        let mut overlay_box = Box::new(FlutterOverlay {
            engine: ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture,
            srv,
            _assets_c: assets_c_temp,
            _icu_c: icu_c_temp,      
            _engine_argv_cs: engine_argv_cs_temp, 
            _dart_argv_cs: dart_argv_cs_temp,    
            _aot_c: aot_c_temp,                 
            _platform_runner_context: Some(platform_context_owner_temp),
            _platform_runner_description: Some(platform_description_owner_temp),
            _custom_task_runners_struct: Some(custom_runners_struct_owner_temp),
            engine_dll: engine_dll_arc.clone(),
        });

        let engine_argv_ptrs: Vec<*const c_char> = overlay_box._engine_argv_cs.iter().map(|c| c.as_ptr()).collect();
        let dart_argv_ptrs: Vec<*const c_char> = overlay_box._dart_argv_cs.iter().map(|c| c.as_ptr()).collect();

        let mut proj_args = FlutterProjectArgs {
            struct_size: std::mem::size_of::<FlutterProjectArgs>(),
            assets_path: overlay_box._assets_c.as_ptr(),
            icu_data_path: overlay_box._icu_c.as_ptr(),
            command_line_argc: engine_argv_ptrs.len() as i32,
            command_line_argv: engine_argv_ptrs.as_ptr(),
            platform_message_callback: Some(simple_platform_message_callback),
            log_message_callback: Some(flutter_log_callback),
            log_tag: FLUTTER_LOG_TAG.as_ptr(),
            custom_task_runners: ptr::null(),
            vm_snapshot_data: ptr::null(),
            vm_snapshot_data_size: 0,
            vm_snapshot_instructions: ptr::null(),
            vm_snapshot_instructions_size: 0,
            isolate_snapshot_data: ptr::null(),
            isolate_snapshot_data_size: 0,
            isolate_snapshot_instructions: ptr::null(),
            isolate_snapshot_instructions_size: 0,
            aot_data: ptr::null_mut(),
            root_isolate_create_callback: None,
            update_semantics_node_callback: None,
            update_semantics_custom_action_callback: None,
            persistent_cache_path: ptr::null(),
            is_persistent_cache_read_only: false,
            vsync_callback: None,
            custom_dart_entrypoint: ptr::null(),
            shutdown_dart_vm_when_done: true,
            compositor: ptr::null(),
            dart_old_gen_heap_size: -1,
            compute_platform_resolved_locale_callback: None,
            dart_entrypoint_argc: dart_argv_ptrs.len() as i32,
            dart_entrypoint_argv: if dart_argv_ptrs.is_empty() { ptr::null() } else { dart_argv_ptrs.as_ptr() },
            on_pre_engine_restart_callback: None,
            update_semantics_callback: None,
            update_semantics_callback2: None,
            channel_update_callback: None,
            main_path__unused__: ptr::null(),
            packages_path__unused__: ptr::null()
        };

        if let Some(aot_c) = &overlay_box._aot_c {
            let source = FlutterEngineAOTDataSource {
                type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
                __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                    elf_path: aot_c.as_ptr(),
                },
            };
            let res = (engine_dll_arc.FlutterEngineCreateAOTData)(&source, &mut proj_args.aot_data);
            if res != FlutterEngineResult_kSuccess {
                error!("[InitOverlay] FlutterEngineCreateAOTData failed: {:?}, path: {}", res, aot_c.to_string_lossy());
                proj_args.aot_data = ptr::null_mut();
                overlay_box._aot_c = None; 
            } else {
                info!("[InitOverlay] FlutterEngineCreateAOTData successful for path: {}", aot_c.to_string_lossy());
            }
        } else {
            proj_args.aot_data = ptr::null_mut();
        }

        if overlay_box._aot_c.is_none() {
            FLUTTER_ASSETS_IS_DEBUG.store(true, Ordering::SeqCst);
        }

        if let Some(ref custom_runners_box_ref) = overlay_box._custom_task_runners_struct {
            proj_args.custom_task_runners = &**custom_runners_box_ref as *const FlutterCustomTaskRunners;
        } else {
            error!(
                "[Flutter:init_overlay] CRITICAL: _custom_task_runners_struct is None in overlay_box. Panicking."
            );
            panic!("Task runner setup failed: _custom_task_runners_struct is None in overlay_box");
        }

        let raw_ptr_to_overlay_data: *mut FlutterOverlay = &mut *overlay_box;

        assert!(
            FLUTTER_OVERLAY_RAW_PTR.is_null(),
            "FLUTTER_OVERLAY_RAW_PTR should be null before being set here"
        );
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr_to_overlay_data;

        let rdr_cfg = build_software_renderer_config();
        let user_data_for_engine_callbacks = raw_ptr_to_overlay_data as *mut c_void;

        let engine_handle = match run_engine(
            FLUTTER_ENGINE_VERSION,
            &rdr_cfg,
            &proj_args,
            user_data_for_engine_callbacks,
            raw_ptr_to_overlay_data,
            engine_dll_arc.clone()
        ) {
            Ok(handle) => {
                info!(
                    "[InitOverlay] engine::run_engine returned successfully. Engine handle: {:?}",
                    handle
                );
                handle
            }
            Err(e) => {
                error!(
                    "[InitOverlay] Failed to initialize and run engine via engine::run_engine: {}",
                    e
                );

                FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut();

                panic!("Engine initialization failed during run_engine: {}", e);
            }
        };

        overlay_box.engine = engine_handle;

        assert_eq!(
            overlay_box.engine, engine_handle,
            "Engine handle in overlay_box mismatch after run_engine"
        );

        update_flutter_window_metrics(engine_handle, width, height, engine_dll_arc.clone());

        set_global_engine_for_platform_messages(engine_handle, engine_dll_arc.clone());

        text_input_set_global_engine(engine_handle, engine_dll_arc.clone());

        overlay_box
    }
}