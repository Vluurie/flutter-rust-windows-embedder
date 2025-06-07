use crate::embedder::{
    self, FlutterEngine, FlutterProjectArgs, FlutterRendererConfig, FlutterWindowMetricsEvent,
};

use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;

use log::{error, info};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::overlay_impl::FlutterOverlay;

pub(crate) fn run_engine(
    version: usize,
    config: &FlutterRendererConfig,
    args: &FlutterProjectArgs,
    user_data: *mut c_void,
    overlay_raw_ptr: *mut FlutterOverlay,
    engine_dll_arc: Arc<FlutterEngineDll>,
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
            (engine_dll_arc.FlutterEngineInitialize)(version, config, args, user_data, &mut engine_handle);

        if init_result != embedder::FlutterEngineResult_kSuccess || engine_handle.is_null() {
            let err_msg = format!(
                "[Engine] FlutterEngineInitialize failed with result: {:?} or engine handle is null.",
                init_result
            );
            error!("{}", err_msg);
            return Err(err_msg);
        }

        (*overlay_raw_ptr).engine = engine_handle;
        (*overlay_raw_ptr).engine_atomic_ptr.store(engine_handle, Ordering::SeqCst);


        let run_result =  (engine_dll_arc.FlutterEngineRunInitialized)(engine_handle);

        if run_result != embedder::FlutterEngineResult_kSuccess {
            let err_msg = format!(
                "[Engine] FlutterEngineRunInitialized failed with result: {:?}",
                run_result
            );
            error!("{}", err_msg);
            

            (engine_dll_arc.FlutterEngineDeinitialize)(engine_handle);
            (*overlay_raw_ptr).engine = ptr::null_mut();
            (*overlay_raw_ptr).engine_atomic_ptr.store(ptr::null_mut(), Ordering::SeqCst);

            return Err(err_msg);
        }
        Ok(engine_handle)
    }
}


pub(crate) fn update_flutter_window_metrics(engine: FlutterEngine, width: u32, height: u32,  engine_dll: Arc<FlutterEngineDll>) {
    if engine.is_null() {
        error!("[Metrics] Attempted to send metrics with a null engine handle.");
        return;
    }
    let mut wm: FlutterWindowMetricsEvent = unsafe { std::mem::zeroed() };
    wm.struct_size = std::mem::size_of::<FlutterWindowMetricsEvent>();
    wm.width = width.try_into().unwrap();
    wm.height = height.try_into().unwrap();
    wm.pixel_ratio = 1.0;
    let r = unsafe { (engine_dll.FlutterEngineSendWindowMetricsEvent)(engine, &wm) };
    if r != embedder::FlutterEngineResult_kSuccess {
        error!(
            "[Metrics] FlutterEngineSendWindowMetricsEvent failed with result: {:?}",
            r
        );
    }
}


