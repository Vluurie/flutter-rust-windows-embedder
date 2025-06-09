use crate::bindings::embedder::{FlutterEngineResult_kSuccess};

use crate::software_renderer::overlay::overlay_impl::
    FlutterOverlay
;
use crate::software_renderer::ticker::task_scheduler::
    ScheduledTask
;

use log::{error};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::{thread, time::Duration};


pub fn start_task_runner(overlay: &mut FlutterOverlay) {
    if overlay.task_runner_thread.is_some() {
        return;
    }

    let engine_dll_for_thread = overlay.engine_dll.clone();
    let task_queue_for_thread = overlay.task_queue_state.clone();
    let name_for_thread = overlay.name.clone();
    let engine_atomic_ptr = overlay.engine_atomic_ptr.clone();

    let handle = thread::Builder::new()
        .name(format!("task_runner_{}", name_for_thread))
        .spawn(move || {
            loop {
                let engine = engine_atomic_ptr.load(Ordering::SeqCst);

                if engine.is_null() {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }

                let mut task_to_run: Option<ScheduledTask> = None;
                let mut wait_duration = Duration::from_millis(100);

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
                        let _ = task_queue_for_thread
                            .condvar
                            .wait_timeout(queue_guard, wait_duration);
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

    let thread_id = handle.thread().id();

    if let Some(context_ref_mut) = &mut overlay._platform_runner_context {
        context_ref_mut.task_runner_thread_id = Some(thread_id);
    } else {
        // This case should ideally not happen if init_overlay correctly initializes _platform_runner_context
        error!("[TaskRunner] CRITICAL: _platform_runner_context is None in FlutterOverlay. Cannot set thread ID.");
    }

    overlay.task_runner_thread = Some(Arc::new(handle));
}