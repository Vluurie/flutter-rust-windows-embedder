use crate::path_utils::load_flutter_build_paths;
use crate::software_renderer::api::RendererType;
use crate::software_renderer::d3d11_compositor::compositor::D3D11Compositor;
use crate::software_renderer::d3d11_compositor::effects::EffectConfig;
use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;

use crate::software_renderer::gl_renderer::angle_interop::{
    AngleInteropState, SendableAngleState, build_opengl_renderer_config,
};
use crate::software_renderer::overlay::d3d::{
    create_compositing_texture, create_d3d_device_on_same_adapter,
    create_shared_texture_and_get_handle, create_srv, create_texture, log_device_adapter_info,
    log_device_creation_flags, log_device_feature_level, log_texture_properties,
};
use crate::software_renderer::overlay::engine::{run_engine, update_flutter_window_metrics};
use crate::software_renderer::overlay::overlay_impl::{
    FLUTTER_LOG_TAG, SendHwnd, SendableFlutterEngine, SendableHandle,
};
use crate::software_renderer::overlay::platform_message_callback::simple_platform_message_callback;
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, flutter_log_callback, maybe_load_aot_path_to_cstring,
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;

use crate::bindings::embedder::{
    self, FlutterCustomTaskRunners, FlutterEngineAOTDataSource,
    FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
    FlutterEngineResult_kSuccess, FlutterProjectArgs, FlutterTaskRunnerDescription,
};
use crate::software_renderer::overlay::semantics_handler::semantics_update_callback;
use crate::software_renderer::ticker::spawn::start_task_runner;
use crate::software_renderer::ticker::task_scheduler::{
    SendableFlutterCustomTaskRunners, SendableFlutterTaskRunnerDescription, TaskQueueState,
    TaskRunnerContext, destroy_task_runner_context_callback, post_task_callback,
    runs_task_on_current_thread_callback,
};
use windows::core::Interface;

use log::{error, info};
use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::{ffi::c_void, path::PathBuf, ptr};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Device1, ID3D11ShaderResourceView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::{DXGI_SWAP_CHAIN_DESC, IDXGIKeyedMutex, IDXGISwapChain};

use super::overlay_impl::FlutterOverlay;

const FLUTTER_ENGINE_VERSION: usize = 1;

pub(crate) fn init_overlay(
    name: String,
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    swap_chain: &IDXGISwapChain,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    dart_args_opt: Option<&[String]>,
    engine_args_opt: Option<&[String]>,
    renderer_type: RendererType,
) -> Box<FlutterOverlay> {
    unsafe {
        if let Ok(device_from_swapchain) = swap_chain.GetDevice::<ID3D11Device>() {
            println!(
                "\n--- Running Device Diagnostics for Overlay '{}' ---",
                name
            );
            log_device_adapter_info(&device_from_swapchain);
            log_device_feature_level(&device_from_swapchain);
            let creation_flags = device_from_swapchain.GetCreationFlags();
            log_device_creation_flags(
                windows::Win32::Graphics::Direct3D11::D3D11_CREATE_DEVICE_FLAG(creation_flags),
            );
            if let Ok(back_buffer_texture) = swap_chain.GetBuffer::<ID3D11Texture2D>(0) {
                log_texture_properties(&back_buffer_texture);
            } else {
                println!(
                    "[DXGI TEXTURE PROBE]   -> ERROR: Failed to get back buffer texture from swap chain."
                );
            }

            println!("--- Device Diagnostics Complete ---\n");
        }

        let engine_dll_load_dir = data_dir.as_deref();
        let engine_dll_arc = FlutterEngineDll::get_for(engine_dll_load_dir).unwrap_or_else(|e| {
            error!(
                "Failed to load flutter_engine.dll from `{:?}`: {:?}",
                engine_dll_load_dir, e
            );
            std::process::exit(1);
        });

        assert!(width > 0 && height > 0, "Width and height must be non-zero");

        let (assets, icu, aot_opt) = load_flutter_build_paths(data_dir.clone());
        let initial_is_debug = aot_opt.is_none();

        let (assets_c_temp, icu_c_temp, engine_argv_cs_temp, dart_argv_cs_temp) =
            build_project_args_and_strings(
                &assets.to_string_lossy(),
                &icu.to_string_lossy(),
                dart_args_opt,
                initial_is_debug,
                engine_args_opt,
            );

        let aot_c_temp = maybe_load_aot_path_to_cstring(aot_opt.as_deref());

        let swap_chain_desc: DXGI_SWAP_CHAIN_DESC = swap_chain.GetDesc().unwrap();
        let hwnd = swap_chain_desc.OutputWindow;

        let rdr_cfg: embedder::FlutterRendererConfig;
        let texture_for_struct: ID3D11Texture2D;
        let srv_for_struct: ID3D11ShaderResourceView;
        let mut gl_internal_linear_texture_for_struct: Option<ID3D11Texture2D> = None;
        let mut pixel_buffer_for_struct: Option<Vec<u8>> = None;
        let mut angle_state_for_struct: Option<SendableAngleState> = None;
        let d3d11_shared_handle_for_struct: Option<SendableHandle> = None;
        let mut keyed_mutex_for_struct: Option<IDXGIKeyedMutex> = None;
        let mut angle_shared_texture_for_struct: Option<ID3D11Texture2D> = None;

        let game_device: &ID3D11Device = device;

        match renderer_type {
            RendererType::Software => {
                info!("[InitOverlay] Using Software Renderer");
                texture_for_struct = create_texture(game_device, width, height);
                srv_for_struct = create_srv(game_device, &texture_for_struct);
                pixel_buffer_for_struct = Some(vec![0; (width * height * 4) as usize]);
                rdr_cfg = build_software_renderer_config();
            }

            RendererType::OpenGL => {
                info!("[InitOverlay] Using two-texture interop model");

                // 1. Initialize ANGLE, which creates its own internal D3D device.
                let angle_state = AngleInteropState::new().expect("Failed to initialize ANGLE");
                let angle_d3d_device = angle_state.get_d3d_device().unwrap();

                // 2. Create the SHARED texture on ANGLE's device. This texture is a RENDER TARGET ONLY.
                let (texture_for_angle, shared_handle) =
                    create_shared_texture_and_get_handle(&angle_d3d_device, width, height)
                        .expect("Failed to create shared ANGLE texture");

                // This is the texture handle Flutter will use in its fbo_callback.
                gl_internal_linear_texture_for_struct = Some(texture_for_angle);

                // 3. Open a handle to ANGLE's shared texture on the GAME's device.
                let angle_texture_on_game_device: ID3D11Texture2D = unsafe {
                    let mut opt = None;
                    game_device
                        .OpenSharedResource(shared_handle, &mut opt)
                        .unwrap();
                    opt.unwrap()
                };

                // This is the handle we will store in our struct to perform the copy from.
                angle_shared_texture_for_struct = Some(angle_texture_on_game_device.clone());

                // 4. Create a LOCAL, non-shared texture on the GAME's device.
                //    This texture is a SHADER RESOURCE ONLY.
                texture_for_struct = create_compositing_texture(game_device, width, height);
                srv_for_struct = create_srv(game_device, &texture_for_struct);

                // 5. Get the mutex from the SHARED texture.
                keyed_mutex_for_struct = angle_texture_on_game_device.cast().ok();

                angle_state_for_struct = Some(SendableAngleState(angle_state));
                rdr_cfg = build_opengl_renderer_config();
            }
        }
        let compositor = D3D11Compositor::new(device);

        let engine_atomic_ptr_instance = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let task_queue_arc = Arc::new(TaskQueueState::new());

        let platform_context_owned_by_overlay = Box::new(TaskRunnerContext {
            task_runner_thread_id: None,
            task_queue: task_queue_arc.clone(),
        });

        let mut overlay_box = Box::new(FlutterOverlay {
            name,
            engine: SendableFlutterEngine(ptr::null_mut()),
            engine_atomic_ptr: engine_atomic_ptr_instance.clone(),
            pixel_buffer: pixel_buffer_for_struct,
            width,
            height,
            visible: true,
            effect_config: EffectConfig::default(),
            x,
            y,
            texture: texture_for_struct,
            srv: srv_for_struct,
            gl_internal_linear_texture: gl_internal_linear_texture_for_struct,
            compositor,
            desired_cursor: Arc::new(Mutex::new(None)),
            task_queue_state: task_queue_arc,
            task_runner_thread: None,
            _assets_c: assets_c_temp,
            _icu_c: icu_c_temp,
            _engine_argv_cs: engine_argv_cs_temp,
            _dart_argv_cs: dart_argv_cs_temp,
            _aot_c: aot_c_temp,
            _platform_runner_context: Some(platform_context_owned_by_overlay),
            _platform_runner_description: None,
            _custom_task_runners_struct: None,
            engine_dll: engine_dll_arc.clone(),
            text_input_state: Arc::new(Mutex::new(None)),
            mouse_buttons_state: AtomicI32::new(0),
            is_mouse_added: AtomicBool::new(false),
            semantics_tree_data: Arc::new(Mutex::new(HashMap::new())),
            is_interactive_widget_hovered: AtomicBool::new(false),
            windows_handler: SendHwnd(hwnd),
            is_debug_build: initial_is_debug,
            angle_shared_texture: angle_shared_texture_for_struct,
            dart_send_port: Arc::new(AtomicI64::new(0)),
            renderer_type,
            angle_state: angle_state_for_struct,
            d3d11_shared_handle: d3d11_shared_handle_for_struct,
            keyed_mutex: keyed_mutex_for_struct,
        });

        let user_data_for_engine: *mut c_void = &mut *overlay_box as *mut _ as *mut c_void;

        start_task_runner(&mut overlay_box);

        let platform_description = FlutterTaskRunnerDescription {
            struct_size: std::mem::size_of::<FlutterTaskRunnerDescription>(),
            user_data: overlay_box
                ._platform_runner_context
                .as_ref()
                .unwrap()
                .as_ref() as *const _ as *mut c_void,
            runs_task_on_current_thread_callback: Some(runs_task_on_current_thread_callback),
            post_task_callback: Some(post_task_callback),
            identifier: 1,
            destruction_callback: Some(destroy_task_runner_context_callback),
        };

        let platform_description_box =
            Box::new(SendableFlutterTaskRunnerDescription(platform_description));

        let custom_task_runners = FlutterCustomTaskRunners {
            struct_size: std::mem::size_of::<FlutterCustomTaskRunners>(),
            platform_task_runner: &platform_description_box.0,
            render_task_runner: &platform_description_box.0,
            thread_priority_setter: None,
        };

        let custom_task_runners_box =
            Box::new(SendableFlutterCustomTaskRunners(custom_task_runners));

        overlay_box._platform_runner_description = Some(platform_description_box);
        overlay_box._custom_task_runners_struct = Some(custom_task_runners_box);

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
            main_path__unused__: ptr::null(),
            packages_path__unused__: ptr::null(),
            icu_data_path: overlay_box._icu_c.as_ptr(),
            command_line_argc: engine_argv_ptrs.len() as i32,
            command_line_argv: if engine_argv_ptrs.is_empty() {
                ptr::null()
            } else {
                engine_argv_ptrs.as_ptr()
            },
            platform_message_callback: Some(simple_platform_message_callback),
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
            custom_task_runners: overlay_box
                ._custom_task_runners_struct
                .as_ref()
                .map_or(ptr::null(), |b| &b.0),
            shutdown_dart_vm_when_done: true,
            compositor: ptr::null(),
            dart_old_gen_heap_size: -1,
            aot_data: ptr::null_mut(),
            compute_platform_resolved_locale_callback: None,
            dart_entrypoint_argc: dart_argv_ptrs.len() as i32,
            dart_entrypoint_argv: if dart_argv_ptrs.is_empty() {
                ptr::null()
            } else {
                dart_argv_ptrs.as_ptr()
            },
            log_message_callback: Some(flutter_log_callback),
            log_tag: FLUTTER_LOG_TAG.as_ptr(),
            on_pre_engine_restart_callback: None,
            update_semantics_callback: None,
            update_semantics_callback2: Some(semantics_update_callback),
            channel_update_callback: None,
        };

        if let Some(aot_c_ref) = &overlay_box._aot_c {
            let source = FlutterEngineAOTDataSource {
                type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
                __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                    elf_path: aot_c_ref.as_ptr(),
                },
            };
            let res = (overlay_box.engine_dll.FlutterEngineCreateAOTData)(
                &source,
                &mut proj_args.aot_data,
            );
            if res != FlutterEngineResult_kSuccess {
                error!(
                    "[InitOverlay] FlutterEngineCreateAOTData failed with code: {:?}, for AOT file: {}",
                    res,
                    aot_c_ref.to_string_lossy()
                );
                proj_args.aot_data = ptr::null_mut();
            }
        }

        overlay_box.is_debug_build = proj_args.aot_data.is_null();

        let raw_ptr_to_overlay_for_run_engine: *mut FlutterOverlay = &mut *overlay_box;
        let engine_run_result = run_engine(
            FLUTTER_ENGINE_VERSION,
            &rdr_cfg,
            &proj_args,
            user_data_for_engine,
            raw_ptr_to_overlay_for_run_engine,
            overlay_box.engine_dll.clone(),
        );

        let engine_handle = match engine_run_result {
            Ok(handle) => handle,
            Err(e) => {
                error!(
                    "[InitOverlay] CRITICAL: Engine initialization failed during run_engine: {}",
                    e
                );
                engine_atomic_ptr_instance.store(ptr::null_mut(), Ordering::SeqCst);
                panic!("Engine initialization failed during run_engine: {}", e);
            }
        };

        (engine_dll_arc.FlutterEngineUpdateSemanticsEnabled)(engine_handle, true);

        overlay_box.engine = SendableFlutterEngine(engine_handle);
        engine_atomic_ptr_instance.store(engine_handle, Ordering::SeqCst);
        assert_eq!(
            overlay_box.engine.0, engine_handle,
            "Engine handle mismatch after storing."
        );

        update_flutter_window_metrics(
            engine_handle,
            x,
            y,
            width,
            height,
            overlay_box.engine_dll.clone(),
        );

        info!(
            "[InitOverlay] Initialization for '{}' completed successfully.",
            overlay_box.name
        );
        overlay_box
    }
}
