use crate::bindings::embedder::{
    FlutterEngineResult_kSuccess, FlutterKeyEvent,
    FlutterKeyEventDeviceType_kFlutterKeyEventDeviceTypeKeyboard, FlutterPlatformMessage,
};

use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;
use crate::software_renderer::ticker::task_scheduler::ScheduledTask;

use log::error;
use std::ffi::{CString, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::{thread, time::Duration};

extern "C" fn key_event_callback(_handled: bool, user_data: *mut c_void) {
    unsafe {
        if user_data.is_null() {
            return;
        }

        drop(Box::from_raw(user_data as *mut u64));
    }
}

pub fn start_task_runner(overlay: &mut FlutterOverlay) {
    if overlay.task_runner_thread.is_some() {
        return;
    }

    let engine_dll_for_thread = overlay.engine_dll.clone();
    let task_queue_for_thread = overlay.task_queue_state.clone();
    let name_for_thread = overlay.name.clone();
    let engine_atomic_ptr = overlay.engine_atomic_ptr.clone();
    let pending_messages_for_thread = overlay.pending_platform_messages.clone();
    let pending_keys_for_thread = overlay.pending_key_events.clone();

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

                if let Ok(mut pending_keys) = pending_keys_for_thread.lock() {
                    while let Some(key_event) = pending_keys.pop_front() {
                        if key_event.physical == 0 && key_event.logical == 0 {
                            continue;
                        }

                        let characters_cstring = CString::new(key_event.characters.clone())
                            .unwrap_or_else(|_| CString::new("").unwrap());

                        let current_time =
                            unsafe { (engine_dll_for_thread.FlutterEngineGetCurrentTime)() };

                        let event_data = FlutterKeyEvent {
                            struct_size: std::mem::size_of::<FlutterKeyEvent>(),
                            timestamp: current_time as f64,
                            type_: key_event.event_type,
                            physical: key_event.physical,
                            logical: key_event.logical,
                            character: characters_cstring.as_ptr(),
                            synthesized: key_event.synthesized,
                            device_type:
                                FlutterKeyEventDeviceType_kFlutterKeyEventDeviceTypeKeyboard,
                        };

                        let physical_key_box = Box::new(key_event.physical);
                        let user_data_ptr = Box::into_raw(physical_key_box) as *mut c_void;

                        let result = unsafe {
                            (engine_dll_for_thread.FlutterEngineSendKeyEvent)(
                                engine,
                                &event_data as *const _,
                                Some(key_event_callback),
                                user_data_ptr,
                            )
                        };

                        if result != FlutterEngineResult_kSuccess {
                            error!(
                                "[TaskRunner] FlutterEngineSendKeyEvent failed: {:?}",
                                result
                            );

                            unsafe {
                                drop(Box::from_raw(user_data_ptr as *mut u64));
                            }
                        }
                    }
                }

                if let Ok(mut pending_msgs) = pending_messages_for_thread.lock() {
                    while let Some(msg) = pending_msgs.pop_front() {
                        if let Ok(channel_cstring) = CString::new(msg.channel.as_str()) {
                            let platform_message = FlutterPlatformMessage {
                                struct_size: std::mem::size_of::<FlutterPlatformMessage>(),
                                channel: channel_cstring.as_ptr(),
                                message: msg.payload_bytes.as_ptr(),
                                message_size: msg.payload_bytes.len(),
                                response_handle: ptr::null(),
                            };

                            let _ = unsafe {
                                (engine_dll_for_thread.FlutterEngineSendPlatformMessage)(
                                    engine,
                                    &platform_message,
                                )
                            };
                        }
                    }
                }
            }
        })
        .expect("Failed to spawn task runner thread");

    let thread_id = handle.thread().id();

    if let Some(context_ref_mut) = &mut overlay._platform_runner_context {
        context_ref_mut.task_runner_thread_id = Some(thread_id);
    } else {
        error!(
            "[TaskRunner] CRITICAL: _platform_runner_context is None in FlutterOverlay. Cannot set thread ID."
        );
    }

    overlay.task_runner_thread = Some(Arc::new(handle));
}
