use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::overlay_impl::FLUTTER_OVERLAY_RAW_PTR;
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::platform_message_callback::set_global_engine_for_platform_messages;
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, maybe_load_aot,
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;

use crate::software_renderer::overlay::engine::{send_initial_metrics};

use crate::embedder::
    FlutterCustomTaskRunners
;
use crate::software_renderer::overlay::textinput::text_input_set_global_engine;

use log::{error, info};
use std::{ffi::c_void, path::PathBuf, ptr};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use super::overlay_impl::FlutterOverlay;

const FLUTTER_ENGINE_VERSION: usize = 1;

pub fn init_overlay(
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Box<FlutterOverlay> {
    unsafe {
        crate::init_logging();

        assert!(width > 0 && height > 0, "Width and height must be non-zero");

        let (assets, icu, aot_opt) = load_flutter_paths(data_dir);
        let texture = create_texture(device, width, height);
        let srv = create_srv(device, &texture);

        let (
            mut proj_args,
            assets_c,
            icu_c,
            argv_cs,
            platform_context_owner,
            platform_description_owner,
            custom_runners_struct_owner,
        ) = build_project_args_and_strings(&assets.to_string_lossy(), &icu.to_string_lossy());

        let aot_c = maybe_load_aot(&mut proj_args, aot_opt.as_deref());

        let mut overlay_box = Box::new(FlutterOverlay {
            engine: ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture,
            srv,
            _assets_c: assets_c,
            _icu_c: icu_c,
            _argv_cs: argv_cs,
            _aot_c: aot_c,
            _platform_runner_context: Some(platform_context_owner),
            _platform_runner_description: Some(platform_description_owner),
            _custom_task_runners_struct: Some(custom_runners_struct_owner),
        });

        let raw_ptr_to_overlay_data: *mut FlutterOverlay = &mut *overlay_box;

        let user_data_for_engine_callbacks = raw_ptr_to_overlay_data as *mut c_void;

        let rdr_cfg = build_software_renderer_config();

        if let Some(ref custom_runners_box_ref) = overlay_box._custom_task_runners_struct {
            proj_args.custom_task_runners =
                &**custom_runners_box_ref as *const FlutterCustomTaskRunners;
        } else {
            error!(
                "[InitOverlay] CRITICAL: _custom_task_runners_struct is None in overlay_box. Panicking."
            );
            panic!("Task runner setup failed: _custom_task_runners_struct is None in overlay_box");
        }

        assert!(
            FLUTTER_OVERLAY_RAW_PTR.is_null(),
            "FLUTTER_OVERLAY_RAW_PTR should be null before being set here"
        );
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr_to_overlay_data;

        let engine_handle = match crate::software_renderer::overlay::engine::run_engine(
            FLUTTER_ENGINE_VERSION,
            &rdr_cfg,
            &proj_args,
            user_data_for_engine_callbacks,
            raw_ptr_to_overlay_data,
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

        assert_eq!(
            overlay_box.engine, engine_handle,
            "Engine handle in overlay_box mismatch after run_engine"
        );

        send_initial_metrics(engine_handle, width as usize, height as usize);

        set_global_engine_for_platform_messages(engine_handle);

        text_input_set_global_engine(engine_handle);

        overlay_box
    }
}
