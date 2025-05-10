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
                info!("[TaskRunner] Thread started. TID: {:?}", current_thread_id);

                // --- Set ThreadId in TaskRunnerContext ---
                // This is crucial for `runs_task_on_current_thread_callback`.
                // It needs to happen once at the beginning of this thread's life.
                unsafe {
                    if FLUTTER_OVERLAY_RAW_PTR.is_null() {
                        error!("[TaskRunner] FLUTTER_OVERLAY_RAW_PTR is null when trying to set ThreadId. Task runner cannot function correctly and will exit.");
                        return;
                    }
                    // Get a mutable reference to the overlay to access its fields.
                    let overlay = &mut *FLUTTER_OVERLAY_RAW_PTR;

                    // Navigate to the TaskRunnerContext. This path depends on how you've stored
                    // the Boxed structures in FlutterOverlay and configured them in project_args.
                    if let Some(custom_runners_box) = overlay._custom_task_runners_struct.as_ref() {
                        let custom_runners_ptr = &**custom_runners_box as *const FlutterCustomTaskRunners;
                        if !(*custom_runners_ptr).platform_task_runner.is_null() {
                            let desc_ptr = (*custom_runners_ptr).platform_task_runner;
                            if !(*desc_ptr).user_data.is_null() {
                                // user_data in FlutterTaskRunnerDescription should point to our TaskRunnerContext.
                                let context = &mut *((*desc_ptr).user_data as *mut TaskRunnerContext);
                                context.task_runner_thread_id = Some(current_thread_id);
                                info!("[TaskRunner] Successfully set current ThreadId ({:?}) in TaskRunnerContext.", current_thread_id);
                            } else {
                                error!("[TaskRunner] user_data in platform_task_runner description is null. Cannot set ThreadId. Exiting.");
                                return;
                            }
                        } else {
                             error!("[TaskRunner] platform_task_runner in custom_runners_struct is null. Cannot set ThreadId. Exiting.");
                             return;
                        }
                    } else {
                        error!("[TaskRunner] _custom_task_runners_struct in FlutterOverlay is None. Cannot set ThreadId. Exiting.");
                        return;
                    }
                }
                // --- End Set ThreadId ---

                // Main task processing loop
                loop {
                    let engine: FlutterEngine;
                    unsafe { // Accessing global static mut
                        if FLUTTER_OVERLAY_RAW_PTR.is_null() {
                            info!("[TaskRunner] Overlay pointer is null. Task runner thread exiting.");
                            break;
                        }
                        let overlay_ref = &*FLUTTER_OVERLAY_RAW_PTR;
                        if overlay_ref.engine.is_null() {
                            // Engine might be shutting down or not yet fully initialized by the main thread.
                            // A more robust system would use a shutdown signal.
                            // For now, if it's null consistently, we exit.
                            warn!("[TaskRunner] Engine pointer in overlay is null. Yielding and will re-check.");
                            thread::sleep(Duration::from_millis(100)); // Wait a bit
                            if (*FLUTTER_OVERLAY_RAW_PTR).engine.is_null() { // Re-check
                                info!("[TaskRunner] Engine pointer still null. Task runner thread exiting.");
                                break;
                            }
                        }
                        engine = (*FLUTTER_OVERLAY_RAW_PTR).engine;
                    }

                    let mut task_to_run_opt: Option<ScheduledTask> = None;
                    // Default wait if queue is empty or next task is far in the future.
                    // This also serves as a maximum wait time for the condvar.
                    let mut wait_duration = Duration::from_millis(100);

                    let task_queue_arc = &*TASK_QUEUE_STATE; // Get a reference to the Arc<TaskQueueState>
                    
                    // Scope for the MutexGuard
                    {
                        let mut queue_guard = match task_queue_arc.queue.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                error!("[TaskRunner] Task queue mutex was poisoned: {:?}. Exiting thread.", poisoned);
                                break;
                            }
                        };

                        let current_time_nanos = unsafe { FlutterEngineGetCurrentTime() };

                        if let Some(next_scheduled_task) = queue_guard.peek() {
                            if next_scheduled_task.target_time <= current_time_nanos {
                                // Task is due or overdue, pop it from the queue.
                                task_to_run_opt = queue_guard.pop();
                            } else {
                                // Next task is in the future, calculate precise wait time.
                                wait_duration = Duration::from_nanos(next_scheduled_task.target_time - current_time_nanos);
                            }
                        }
                        
                        // If no task is immediately runnable, wait on the condition variable.
                        if task_to_run_opt.is_none() {
                            // Cap the wait duration to avoid excessively long sleeps if condvar logic has issues.
                            let final_wait_duration = std::cmp::min(wait_duration, Duration::from_secs(1));
                            // debug!("[TaskRunner] Waiting for {:?} or on condvar notify.", final_wait_duration);
                            
                            // `wait_timeout` atomically releases the lock, waits, and reacquires the lock.
                            match task_queue_arc.condvar.wait_timeout(queue_guard, final_wait_duration) {
                                Ok((_new_guard, timeout_result)) => {
                                    // queue_guard is reacquired here.
                                    if timeout_result.timed_out() {
                                        // debug!("[TaskRunner] Woke up from timeout.");
                                    } else {
                                        // debug!("[TaskRunner] Woke up from condvar notification.");
                                    }
                                    // The loop will now re-evaluate the queue with the reacquired lock.
                                }
                                Err(poisoned) => {
                                    error!("[TaskRunner] Condvar wait_timeout failed (mutex poisoned): {:?}. Exiting thread.", poisoned);
                                    break; 
                                }
                            }
                        }
                        // MutexGuard for `queue_guard` is dropped here, releasing the lock.
                    }


                    // Execute the task if one was retrieved. This is done outside the mutex lock.
                    if let Some(scheduled_task) = task_to_run_opt {
                        debug!(
                            "[TaskRunner] Executing TaskId={}, TargetTime={}",
                            scheduled_task.task.0.task, // Access inner FlutterTask's u64 id
                            scheduled_task.target_time
                        );
                        // Pass the original FlutterTask (task.0) from SafeFlutterTask to the engine.
                        let result = unsafe { FlutterEngineRunTask(engine, &scheduled_task.task.0) };
                        if result != FlutterEngineResult_kSuccess {
                            error!(
                                "[TaskRunner] FlutterEngineRunTask for TaskId {} failed with result: {:?}",
                                scheduled_task.task.0.task, result
                            );
                        }
                    }
                    // A very short sleep can be added here if there are concerns about
                    // tight loops under specific edge conditions, but generally, the condvar
                    // should manage the waiting efficiently.
                    // thread::sleep(Duration::from_micros(10)); 
                } // end loop
                info!("[TaskRunner] Task runner thread has exited. TID: {:?}", current_thread_id);
            })
            .expect("Failed to spawn task runner thread");
        info!("[TaskRunner] Task runner thread spawn successfully initiated.");
    });
}
