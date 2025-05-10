use crate::embedder::FlutterEngineRunTask;
use crate::software_renderer::overlay::{FlutterOverlay, FLUTTER_OVERLAY_RAW_PTR};
use std::sync::Once;
use std::{thread, time::Duration};

// Ensure we only ever spawn one runner.
static START: Once = Once::new();

/// Call this once (e.g. at the end of your init). It will spawn
/// a thread that continually calls FlutterEngineRunTask(engine, null)
/// at roughly 60 Hz until the engine pointer goes null.
pub fn spawn_task_runner() {
    START.call_once(|| {
        // Capture the raw pointer as a usize.
        let ptr_val = unsafe { FLUTTER_OVERLAY_RAW_PTR } as usize;

        thread::spawn(move || {
            loop {
                let overlay_ptr = ptr_val as *mut FlutterOverlay;
                if overlay_ptr.is_null() {
                    break;
                }
                let engine = unsafe { (*overlay_ptr).engine };
                if engine.is_null() {
                    break;
                }
                unsafe { FlutterEngineRunTask(engine, std::ptr::null_mut()) };
                thread::sleep(Duration::from_millis(16));
            }
        });
    });
}
