use crate::embedder::{FlutterEngine, FlutterEngineResult_kSuccess};

use crate::software_renderer::overlay::overlay_impl::
    FlutterOverlay
;
use crate::software_renderer::ticker::task_scheduler::
    ScheduledTask
;

use log::{error, info};
use std::{thread, time::Duration};

pub fn start_task_runner(overlay: &mut FlutterOverlay) {
    if overlay.task_runner_thread.is_some() {
        info!(
            "[TaskRunner] Task runner for '{}' runs already.",
            overlay.name
        );
        return;
    }

    info!(
        "[TaskRunner] Spawning task runner for overlay '{}'...",
        overlay.name
    );

    let engine_dll_for_thread = overlay.engine_dll.clone();
    let task_queue_for_thread = overlay.task_queue_state.clone();
    let name_for_thread = overlay.name.clone();

    let engine_addr = overlay.engine as usize; // HACK: We say to the compiler ... all is fine by making it before adding it to the thread an usize.

    let handle = thread::Builder::new()
        .name(format!("task_runner_{}", name_for_thread))
        .spawn(move || {
            loop {
                let engine = engine_addr as FlutterEngine;

                if engine.is_null() {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }

                let mut task_to_run: Option<ScheduledTask> = None;
                let mut wait_duration = Duration::from_millis(2);

                {
                    let mut queue_guard = task_queue_for_thread.queue.lock().unwrap();
                    let now = unsafe { (engine_dll_for_thread.FlutterEngineGetCurrentTime)() };

                    if let Some(task) = queue_guard.peek() {
                        if task.target_time <= now {
                            task_to_run = queue_guard.pop();
                        } else {
                            let nanos_until_due = task.target_time - now;
                            wait_duration = Duration::from_nanos(nanos_until_due);
                        }
                    }

                    if task_to_run.is_none() {
                        let wait_cap = Duration::from_millis(8);
                        let final_wait = std::cmp::min(wait_duration, wait_cap);
                        let _ = task_queue_for_thread
                            .condvar
                            .wait_timeout(queue_guard, final_wait);
                    }
                }

                if let Some(scheduled_task) = task_to_run {
                    let result = unsafe {
                        (engine_dll_for_thread.FlutterEngineRunTask)(engine, &scheduled_task.task.0)
                    };
                    if result != FlutterEngineResult_kSuccess {
                        error!("[TaskRunner] FlutterEngineRunTask failed: {:?}", result);
                    }
                }
            }
        })
        .expect("Failed to spawn task runner thread");

    overlay.task_runner_thread = Some(std::sync::Arc::new(handle));
}
