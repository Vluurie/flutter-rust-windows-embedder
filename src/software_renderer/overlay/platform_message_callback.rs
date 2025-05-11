use crate::embedder;
use log::{error, info};
use std::ffi::{CStr, c_void};
use std::ptr;
use std::str;

static mut GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES: Option<embedder::FlutterEngine> = None;

#[allow(dead_code)]
pub unsafe fn set_global_engine_for_platform_messages(engine: embedder::FlutterEngine) {
    GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES = Some(engine);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_platform_message_callback(
    platform_message: *const embedder::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    if !user_data.is_null() {
        info!(
            "[PLATFORM_MSG_CALLBACK] Received message with user_data: {:?}",
            user_data
        );
    } else {
        info!("[PLATFORM_MSG_CALLBACK] Received message with null user_data.");
    }

    if platform_message.is_null() {
        error!("[PLATFORM_MSG_CALLBACK] Received null platform_message pointer.");
        return;
    }

    let message = &*platform_message;

    let channel_name = if message.channel.is_null() {
        "<unknown_channel>".to_string()
    } else {
        CStr::from_ptr(message.channel)
            .to_string_lossy()
            .into_owned()
    };

    info!(
        "[PLATFORM_MSG_CALLBACK] Received message on channel: '{}', size: {}",
        channel_name, message.message_size
    );

    if message.message_size > 0 && !message.message.is_null() {
        let message_content_slice =
            std::slice::from_raw_parts(message.message, message.message_size);

        match str::from_utf8(message_content_slice) {
            Ok(message_str) => {
                info!(
                    "[PLATFORM_MSG_CALLBACK] Message content (UTF-8): {}",
                    message_str
                );
            }
            Err(_) => {
                let max_bytes_to_log = 64;
                let end_index = std::cmp::min(message_content_slice.len(), max_bytes_to_log);
                let bytes_to_log = &message_content_slice[..end_index];

                info!(
                    "[PLATFORM_MSG_CALLBACK] Message content (raw bytes, first {} of {}): {:?}",
                    bytes_to_log.len(),
                    message_content_slice.len(),
                    bytes_to_log
                );
            }
        }
    } else if message.message_size == 0 {
        info!("[PLATFORM_MSG_CALLBACK] Message content is empty.");
    }

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
                    "[PLATFORM_MSG_CALLBACK] Global engine handle is null. Cannot send response for channel: '{}'",
                    channel_name
                );
            }
        } else {
            error!(
                "[PLATFORM_MSG_CALLBACK] GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES is None. Cannot send response for channel: '{}'",
                channel_name
            );
        }
    }
}
