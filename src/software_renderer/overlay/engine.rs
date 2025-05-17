use crate::embedder::{
    self, FlutterEngine, FlutterProjectArgs, FlutterRendererConfig, FlutterWindowMetricsEvent,
};

use crate::software_renderer::ticker::spawn::spawn_task_runner;

use log::{error, info};
use std::ffi::c_void;
use std::ptr;

use super::overlay_impl::FlutterOverlay;

pub fn run_engine(
    version: usize,
    config: &FlutterRendererConfig,
    args: &FlutterProjectArgs,
    user_data: *mut c_void,
    overlay_raw_ptr: *mut FlutterOverlay,
) -> Result<FlutterEngine, String> {
    unsafe {
        let mut engine_handle: FlutterEngine = ptr::null_mut();

        if overlay_raw_ptr.is_null() {
            let err_msg =
                "[Engine] overlay_raw_ptr is null. Cannot proceed with engine initialization."
                    .to_string();
            error!("{}", err_msg);
            return Err(err_msg);
        }

        assert_eq!(
            user_data as *mut FlutterOverlay, overlay_raw_ptr,
            "user_data and overlay_raw_ptr should match if user_data is the overlay pointer"
        );

        let init_result =
            embedder::FlutterEngineInitialize(version, config, args, user_data, &mut engine_handle);

        if init_result != embedder::FlutterEngineResult_kSuccess || engine_handle.is_null() {
            let err_msg = format!(
                "[Engine] FlutterEngineInitialize failed with result: {:?} or engine handle is null.",
                init_result
            );
            error!("{}", err_msg);
            return Err(err_msg);
        }

        (*overlay_raw_ptr).engine = engine_handle;

        spawn_task_runner();

        let run_result = embedder::FlutterEngineRunInitialized(engine_handle);

        if run_result != embedder::FlutterEngineResult_kSuccess {
            let err_msg = format!(
                "[Engine] FlutterEngineRunInitialized failed with result: {:?}",
                run_result
            );
            error!("{}", err_msg);

            embedder::FlutterEngineDeinitialize(engine_handle);

            (*overlay_raw_ptr).engine = ptr::null_mut();
            return Err(err_msg);
        }
        Ok(engine_handle)
    }
}

pub fn send_initial_metrics(engine: FlutterEngine, width: usize, height: usize) {
    if engine.is_null() {
        error!("[Metrics] Attempted to send metrics with a null engine handle.");
        return;
    }
    let mut wm: FlutterWindowMetricsEvent = unsafe { std::mem::zeroed() };
    wm.struct_size = std::mem::size_of::<FlutterWindowMetricsEvent>();
    wm.width = width;
    wm.height = height;
    wm.pixel_ratio = 1.0;
    info!(
        "[Metrics] Sending window metrics: {}x{} (PixelRatio: {}) for engine {:?}",
        width, height, wm.pixel_ratio, engine
    );
    let r = unsafe { embedder::FlutterEngineSendWindowMetricsEvent(engine, &wm) };
    if r != embedder::FlutterEngineResult_kSuccess {
        error!(
            "[Metrics] FlutterEngineSendWindowMetricsEvent failed with result: {:?}",
            r
        );
    } else {
        info!("[Metrics] FlutterEngineSendWindowMetricsEvent successful.");
    }
}
