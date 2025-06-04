use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::engine::{run_engine, update_flutter_window_metrics};
use crate::software_renderer::overlay::overlay_impl::FLUTTER_LOG_TAG;
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::platform_message_callback::simple_platform_message_callback;
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, flutter_log_callback, maybe_load_aot_path_to_cstring,
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;

use crate::embedder::{
    self, FlutterCustomTaskRunners, FlutterEngineAOTDataSource,
    FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
    FlutterEngineResult_kSuccess, FlutterProjectArgs,
};
use crate::software_renderer::overlay::semantics_handler::semantics_update_callback;

use log::{error, info};
use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::{Arc, Mutex};
use std::{ffi::c_void, path::PathBuf, ptr};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Dxgi::IDXGISwapChain;

use super::overlay_impl::FlutterOverlay;

const FLUTTER_ENGINE_VERSION: usize = 1;

pub(crate) fn init_overlay(
    name: String,
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    swap_chain: &IDXGISwapChain,
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

        assert!(width > 0 && height > 0, "Width and height must be non-zero");

        let (assets, icu, aot_opt) = load_flutter_paths(data_dir.clone());
        let current_instance_is_debug = aot_opt.is_none();
        let texture: windows::Win32::Graphics::Direct3D11::ID3D11Texture2D =
            create_texture(device, width, height);
        let srv = create_srv(device, &texture);

        let (
            assets_c_temp,
            icu_c_temp,
            engine_argv_cs_temp,
            dart_argv_cs_temp,
            platform_context_owner_temp,
            platform_description_owner_temp,
            custom_runners_struct_owner_temp,
            task_queue_state_from_build,
        ) = build_project_args_and_strings(
            &assets.to_string_lossy(),
            &icu.to_string_lossy(),
            dart_args_opt,
            current_instance_is_debug,
        );

        let mut desc = windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_DESC::default();
        swap_chain
            .GetDesc(&mut desc)
            .expect("Failed to get swap chain description");
        let extracted_windows_handler: HWND = desc.OutputWindow;

        let aot_c_temp = maybe_load_aot_path_to_cstring(aot_opt.as_deref());
        let mut overlay_box = Box::new(FlutterOverlay {
            name: name,
            engine: ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture,
            srv,
            desired_cursor: Arc::new(Mutex::new(None)),
            task_queue_state: task_queue_state_from_build,
            task_runner_thread: None,
            _assets_c: assets_c_temp,
            _icu_c: icu_c_temp,
            _engine_argv_cs: engine_argv_cs_temp,
            _dart_argv_cs: dart_argv_cs_temp,
            _aot_c: aot_c_temp,
            _platform_runner_context: Some(platform_context_owner_temp),
            _platform_runner_description: Some(platform_description_owner_temp),
            _custom_task_runners_struct: Some(custom_runners_struct_owner_temp),
            engine_dll: engine_dll_arc.clone(),
            text_input_state: Arc::new(Mutex::new(None)),
            mouse_buttons_state: AtomicI32::new(0),
            is_mouse_added: AtomicBool::new(false),
            semantics_tree_data: Arc::new(Mutex::new(HashMap::new())),
            is_interactive_widget_hovered: AtomicBool::new(false),
            windows_handler: extracted_windows_handler,
            is_debug_build: current_instance_is_debug,
        });

        let raw_ptr_to_overlay_data: *mut FlutterOverlay = &mut *overlay_box;
        let user_data_for_callbacks = raw_ptr_to_overlay_data as *mut c_void;

        if let Some(platform_desc_box) = overlay_box._platform_runner_description.as_mut() {
            platform_desc_box.user_data = user_data_for_callbacks;
        }

        let engine_argv_ptrs: Vec<*const c_char> = overlay_box
            ._engine_argv_cs
            .iter()
            .map(|c| c.as_ptr())
            .collect();
        let dart_argv_ptrs: Vec<*const c_char> = overlay_box
            ._dart_argv_cs
            .iter()
            .map(|c| c.as_ptr())
            .collect();

        let mut proj_args = FlutterProjectArgs {
            struct_size: std::mem::size_of::<FlutterProjectArgs>(),
            assets_path: overlay_box._assets_c.as_ptr(),
            icu_data_path: overlay_box._icu_c.as_ptr(),
            command_line_argc: engine_argv_ptrs.len() as i32,
            command_line_argv: if engine_argv_ptrs.is_empty() {
                ptr::null()
            } else {
                engine_argv_ptrs.as_ptr()
            },
            platform_message_callback: Some(simple_platform_message_callback),
            log_message_callback: Some(flutter_log_callback),
            log_tag: FLUTTER_LOG_TAG.as_ptr(),
            custom_task_runners: if let Some(ref runners_box) =
                overlay_box._custom_task_runners_struct
            {
                &**runners_box as *const FlutterCustomTaskRunners
            } else {
                ptr::null()
            },
            aot_data: ptr::null_mut(),
            dart_entrypoint_argc: dart_argv_ptrs.len() as i32,
            dart_entrypoint_argv: if dart_argv_ptrs.is_empty() {
                ptr::null()
            } else {
                dart_argv_ptrs.as_ptr()
            },
            shutdown_dart_vm_when_done: true,
            dart_old_gen_heap_size: -1,
            vm_snapshot_data: ptr::null(),
            vm_snapshot_data_size: 0,
            vm_snapshot_instructions: ptr::null(),
            vm_snapshot_instructions_size: 0,
            isolate_snapshot_data: ptr::null(),
            isolate_snapshot_data_size: 0,
            isolate_snapshot_instructions: ptr::null(),
            isolate_snapshot_instructions_size: 0,
            root_isolate_create_callback: None,
            update_semantics_node_callback: None,
            update_semantics_custom_action_callback: None,
            persistent_cache_path: ptr::null(),
            is_persistent_cache_read_only: false,
            vsync_callback: None,
            custom_dart_entrypoint: ptr::null(),
            compositor: ptr::null(),
            compute_platform_resolved_locale_callback: None,
            on_pre_engine_restart_callback: None,
            update_semantics_callback: None,
            update_semantics_callback2: Some(semantics_update_callback),
            channel_update_callback: None,
            main_path__unused__: ptr::null(),
            packages_path__unused__: ptr::null(),
        };

        if let Some(aot_c) = &overlay_box._aot_c {
            let source = FlutterEngineAOTDataSource {
                type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
                __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                    elf_path: aot_c.as_ptr(),
                },
            };
            let res = (overlay_box.engine_dll.FlutterEngineCreateAOTData)(
                &source,
                &mut proj_args.aot_data,
            );
            if res != FlutterEngineResult_kSuccess {
                error!(
                    "[InitOverlay] FlutterEngineCreateAOTData failed with result {:?}, for AOT path: {}",
                    res,
                    aot_c.to_string_lossy()
                );
                proj_args.aot_data = ptr::null_mut();
                overlay_box._aot_c = None;
            } else {
                info!(
                    "[InitOverlay] FlutterEngineCreateAOTData successful for AOT path: {}. proj_args.aot_data set to {:p}",
                    aot_c.to_string_lossy(),
                    proj_args.aot_data
                );
            }
        } else {
            proj_args.aot_data = ptr::null_mut();
        }

        if proj_args.aot_data.is_null() && overlay_box._aot_c.is_none() {
            overlay_box.is_debug_build = true;
        } else if !proj_args.aot_data.is_null() && overlay_box._aot_c.is_some() {
            overlay_box.is_debug_build = false;
        }

        overlay_box.is_debug_build = proj_args.aot_data.is_null();

        let rdr_cfg = build_software_renderer_config();
        let engine_run_result = run_engine(
            FLUTTER_ENGINE_VERSION,
            &rdr_cfg,
            &proj_args,
            user_data_for_callbacks,
            raw_ptr_to_overlay_data,
            overlay_box.engine_dll.clone(),
        );

        let engine_handle = match engine_run_result {
            Ok(handle) => handle,
            Err(e) => {
                panic!("Engine initialization failed during run_engine: {}", e);
            }
        };

        (engine_dll_arc.FlutterEngineUpdateSemanticsEnabled)(engine_handle, true);
        overlay_box.engine = engine_handle;
        overlay_box.start_task_runner();

        assert_eq!(
            overlay_box.engine, engine_handle,
            "Engine handle in overlay_box mismatch after run_engine"
        );

        update_flutter_window_metrics(engine_handle, width, height, overlay_box.engine_dll.clone());

        overlay_box
    }
}
