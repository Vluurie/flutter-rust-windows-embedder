use crate::embedder;
use log::{error, info};
use std::ffi::{CStr, c_void};
use std::ptr;

static mut GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES: Option<embedder::FlutterEngine> = None;

pub unsafe fn set_global_engine_for_platform_messages(engine: embedder::FlutterEngine) {
    unsafe { GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES = Some(engine) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_platform_message_callback(
    platform_message: *const embedder::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    info!("[PLATFORM_MSG_CALLBACK] Received message: {:?}", user_data);
    unsafe {
        if platform_message.is_null() {
            error!("[PLATFORM_MSG_CALLBACK] Received null message pointer.");
            return;
        }

        //  if user_data was a pointer to your AppState struct containing the engine:
        // let app_state = &*(user_data as *const YourAppState);
        // let engine_from_user_data = app_state.engine;
        if !user_data.is_null() {
            info!("[PLATFORM_MSG_CALLBACK] user_data pointer: {:?}", user_data);
        }

        let message = &*platform_message;
        let channel_name_c_str = CStr::from_ptr(message.channel);
        let channel_name = channel_name_c_str.to_string_lossy();

        info!(
            "[PLATFORM_MSG_CALLBACK] Received message on channel: '{}', size: {}",
            channel_name, message.message_size
        );

        if !message.response_handle.is_null() {
            let engine_opt = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES;

            if let Some(engine) = engine_opt {
                if !engine.is_null() {
                    info!(
                        "[PLATFORM_MSG_CALLBACK] Sending empty response for channel: '{}'",
                        channel_name
                    );
                    let result = embedder::FlutterEngineSendPlatformMessageResponse(
                        engine,
                        message.response_handle,
                        ptr::null(),
                        0,
                    );
                    if result != embedder::FlutterEngineResult_kSuccess {
                        error!(
                            "[PLATFORM_MSG_CALLBACK] Failed to send response for channel '{}': {:?}",
                            channel_name, result
                        );
                    }
                } else {
                    error!(
                        "[PLATFORM_MSG_CALLBACK] Engine handle is null. Cannot send response for channel: '{}'",
                        channel_name
                    );
                }
            } else {
                error!(
                    "[PLATFORM_MSG_CALLBACK] GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES not set. Cannot send response for channel: '{}'",
                    channel_name
                );
            }
        }
    }
}
