// Likely in software_renderer/ticker/spawn.rs

use crate::embedder::{
    FlutterEngine, // Assuming FlutterEngine type is accessible
    FlutterEngineGetCurrentTime,
    FlutterEngineRunTask,
    FlutterEngineResult_kSuccess,
    FlutterTaskRunnerDescription, // Needed for context access path
    FlutterCustomTaskRunners,    // Needed for context access path
};
use crate::software_renderer::overlay::{
    FLUTTER_OVERLAY_RAW_PTR, 
    FlutterOverlay, // Your main overlay struct
};
// Import from your task_scheduler.rs
use crate::software_renderer::ticker::task_scheduler::{
    TASK_QUEUE_STATE, // The global task queue and condvar
    TaskRunnerContext,  // The context struct we need to update
    ScheduledTask,      // The struct wrapping a task and its target time
};

use std::sync::Once;
use std::{thread, time::Duration, ptr};
use log::{info, error, warn, debug};

// Ensures the task runner thread is only spawned once.
static START_TASK_RUNNER_THREAD: Once = Once::new();

pub fn spawn_task_runner() {
    START_TASK_RUNNER_THREAD.call_once(|| {
        info!("[TaskRunner] Initializing and spawning the task runner thread...");

        thread::Builder::new()
            .name("flutter_task_runner".to_string())
            .spawn(move || {
                let current_thread_id = thread::current().id();

                // Initialize thread context for Flutter
                unsafe {
                    if FLUTTER_OVERLAY_RAW_PTR.is_null() {
                        error!("[TaskRunner] FLUTTER_OVERLAY_RAW_PTR is null. Exiting.");
                        return;
                    }
                    let overlay = &mut *FLUTTER_OVERLAY_RAW_PTR;

                    if let Some(custom_runners_box) = overlay._custom_task_runners_struct.as_ref() {
                        let custom_runners_ptr = &**custom_runners_box;
                        if !custom_runners_ptr.platform_task_runner.is_null() {
                            let desc_ptr = custom_runners_ptr.platform_task_runner;
                            if !(*desc_ptr).user_data.is_null() {
                                let context = &mut *((*desc_ptr).user_data as *mut TaskRunnerContext);
                                context.task_runner_thread_id = Some(current_thread_id);
                            } else {
                                error!("[TaskRunner] user_data is null. Exiting.");
                                return;
                            }
                        } else {
                            error!("[TaskRunner] platform_task_runner is null. Exiting.");
                            return;
                        }
                    } else {
                        error!("[TaskRunner] _custom_task_runners_struct is None. Exiting.");
                        return;
                    }
                }

                // Task loop
                let task_queue_arc = &*TASK_QUEUE_STATE;
                let retry_engine_delay = Duration::from_millis(10);

                loop {
                    // Check for engine validity
                    let engine = unsafe {
                        if FLUTTER_OVERLAY_RAW_PTR.is_null() {
                            info!("[TaskRunner] Overlay ptr is null. Exiting.");
                            break;
                        }
                        let overlay = &*FLUTTER_OVERLAY_RAW_PTR;
                        if overlay.engine.is_null() {
                            thread::sleep(retry_engine_delay);
                            continue;
                        }
                        overlay.engine
                    };

                    let mut task_to_run: Option<ScheduledTask> = None;
                    let mut wait_duration = Duration::from_millis(2); // Much tighter wait window

                    {
                        let mut queue_guard = match task_queue_arc.queue.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                error!("[TaskRunner] Queue mutex poisoned: {:?}. Exiting.", poisoned);
                                break;
                            }
                        };

                        let now = unsafe { FlutterEngineGetCurrentTime() };

                        if let Some(task) = queue_guard.peek() {
                            if task.target_time <= now {
                                task_to_run = queue_guard.pop();
                            } else {
                                let nanos_until_due = task.target_time - now;
                                wait_duration = Duration::from_nanos(nanos_until_due);
                            }
                        }

                        if task_to_run.is_none() {
                            let wait_cap = Duration::from_millis(8); // Cap max wait to 8ms for interactivity
                            let final_wait = std::cmp::min(wait_duration, wait_cap);

                            match task_queue_arc.condvar.wait_timeout(queue_guard, final_wait) {
                                Ok((_guard, _)) => {}
                                Err(poisoned) => {
                                    error!("[TaskRunner] Condvar wait poisoned: {:?}. Exiting.", poisoned);
                                    break;
                                }
                            }
                        }
                    }

                    if let Some(scheduled_task) = task_to_run {
                        let now = unsafe { FlutterEngineGetCurrentTime() };

                        // Busy-wait if we're just slightly early (sub-millisecond precision)
                        let slack_ns = 500_000; // 500 µs
                        if scheduled_task.target_time > now && (scheduled_task.target_time - now) < slack_ns {
                            while unsafe { FlutterEngineGetCurrentTime() } < scheduled_task.target_time {}
                        }

                        // Dispatch the task to Flutter engine
                        let result = unsafe { FlutterEngineRunTask(engine, &scheduled_task.task.0) };
                        if result != FlutterEngineResult_kSuccess {
                            error!(
                                "[TaskRunner] FlutterEngineRunTask for TaskId {} failed: {:?}",
                                scheduled_task.task.0.task, result
                            );
                        }
                    }
                }

                info!("[TaskRunner] Exiting thread: {:?}", current_thread_id);
            })
            .expect("Failed to spawn task runner thread");

        info!("[TaskRunner] Task runner thread spawned successfully.");
    });
}

