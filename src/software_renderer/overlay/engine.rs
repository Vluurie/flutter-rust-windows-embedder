use crate::embedder;
use std::ffi::c_void;
use log::{info, error};

/// Run FlutterEngineRun, panic on failure.
pub fn run_engine(
    version: usize,
    cfg: &embedder::FlutterRendererConfig,
    args: &embedder::FlutterProjectArgs,
    user_data: *mut c_void,
) -> embedder::FlutterEngine {
    let mut engine = std::ptr::null_mut();
    let r = unsafe { embedder::FlutterEngineRun(version, cfg, args, user_data, &mut engine) };
    if r != embedder::FlutterEngineResult_kSuccess {
        panic!("FlutterEngineRun failed: {:?}", r);
    }
    info!("FlutterEngineRun succeeded");
    engine
}

/// Send initial window metrics; log errors.
pub fn send_initial_metrics(
    engine: embedder::FlutterEngine,
    width: usize,
    height: usize,
) {
    let mut wm: embedder::FlutterWindowMetricsEvent = unsafe { std::mem::zeroed() };
    wm.struct_size = std::mem::size_of::<embedder::FlutterWindowMetricsEvent>();
    wm.width = width;
    wm.height = height;
    wm.pixel_ratio = 1.0;
    let r = unsafe { embedder::FlutterEngineSendWindowMetricsEvent(engine, &wm) };
    if r != embedder::FlutterEngineResult_kSuccess {
        error!("SendWindowMetricsEvent failed: {:?}", r);
    }
}
