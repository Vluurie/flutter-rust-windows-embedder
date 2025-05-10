use crate::embedder::{FlutterEngineGetCurrentTime, FlutterEngineRunTask};
use crate::software_renderer::overlay::FLUTTER_OVERLAY_RAW_PTR;
use std::sync::Once;
use std::{thread, time::Duration};

static START: Once = Once::new();

pub fn spawn_task_runner() {
    START.call_once(|| {
        let _ptr_val = unsafe { FLUTTER_OVERLAY_RAW_PTR } as usize;

        thread::spawn(move || {
            loop {
                let overlay_ptr =
                    unsafe { crate::software_renderer::overlay::FLUTTER_OVERLAY_RAW_PTR } as usize
                        as *mut crate::software_renderer::overlay::FlutterOverlay;
                if overlay_ptr.is_null() {
                    log::info!("[TaskRunner] Overlay pointer is null, exiting task runner thread.");
                    break;
                }
                let engine = unsafe { (*overlay_ptr).engine };
                if engine.is_null() {
                    log::info!("[TaskRunner] Engine pointer is null, exiting task runner thread.");
                    break;
                }

                let next_task_time_i32: i32 =
                    unsafe { FlutterEngineRunTask(engine, std::ptr::null_mut()) };

                let current_time_nanos: u64 = unsafe { FlutterEngineGetCurrentTime() };

                if next_task_time_i32 == 0 {
                    thread::sleep(Duration::from_millis(16));
                } else if next_task_time_i32 > 0 {
                    let next_task_nanos_u64 = next_task_time_i32 as u64;

                    if next_task_nanos_u64 > current_time_nanos {
                        let wait_duration_nanos = next_task_nanos_u64 - current_time_nanos;
                        let wait_duration = Duration::from_nanos(wait_duration_nanos);

                        let max_sleep = Duration::from_millis(100);
                        thread::sleep(std::cmp::min(wait_duration, max_sleep));
                    } else {
                    }
                } else {
                    log::error!(
                        "[TaskRunner] FlutterEngineRunTask returned an invalid negative value: {}",
                        next_task_time_i32
                    );
                    thread::sleep(Duration::from_millis(16));
                }
            }
        });
    });
}
