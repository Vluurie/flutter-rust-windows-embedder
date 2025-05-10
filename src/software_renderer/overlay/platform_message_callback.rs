// In your platform_message_callback.rs (or equivalent file)

use crate::embedder; // For FlutterPlatformMessage, FlutterEngineSendPlatformMessageResponse, FlutterEngineResult_kSuccess
use log::{error, info}; // Assuming 'warn' is not used in this version
use std::ffi::{CStr, c_void};
use std::ptr;

// Definition for GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES as per your snippet
// This should be defined in the same module or be accessible.
static mut GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES: Option<embedder::FlutterEngine> = None;

// Function to set the global engine handle, as per your snippet
// This function would be called from your main initialization logic (e.g., in init_overlay)
#[allow(dead_code)] // To prevent warnings if not called from elsewhere in this snippet
pub unsafe fn set_global_engine_for_platform_messages(engine: embedder::FlutterEngine) {
    GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES = Some(engine);
}

#[unsafe(no_mangle)] // Using your specified attribute
pub unsafe extern "C" fn simple_platform_message_callback(
    platform_message: *const embedder::FlutterPlatformMessage,
    user_data: *mut c_void, // user_data is present but not used in this version to get the engine
) {
    // Logging user_data if it's not null, as in your version
    if !user_data.is_null() {
        info!("[PLATFORM_MSG_CALLBACK] Received message with user_data: {:?}", user_data);
    } else {
        info!("[PLATFORM_MSG_CALLBACK] Received message with null user_data.");
    }

    if platform_message.is_null() {
        error!("[PLATFORM_MSG_CALLBACK] Received null platform_message pointer.");
        return;
    }

    let message = &*platform_message; // Dereference the message pointer

    // Handle channel name
    let channel_name = if message.channel.is_null() {
        "<unknown_channel>".to_string() // Handle null channel name gracefully
    } else {
        CStr::from_ptr(message.channel).to_string_lossy().into_owned()
    };

    info!(
        "[PLATFORM_MSG_CALLBACK] Received message on channel: '{}', size: {}",
        channel_name, message.message_size
    );

    // Handle message content (example, your original code didn't show specific processing)
    // if message.message_size > 0 && !message.message.is_null() {
    //     let message_content_slice = std::slice::from_raw_parts(message.message, message.message_size);
    //     if let Ok(message_str) = std::str::from_utf8(message_content_slice) {
    //         info!("[PLATFORM_MSG_CALLBACK] Message content: {}", message_str);
    //     } else {
    //         info!("[PLATFORM_MSG_CALLBACK] Message content (raw bytes): {:?}", message_content_slice);
    //     }
    // }

    // Respond if a response_handle is present
    if !message.response_handle.is_null() {
        let engine_opt = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES; // Access the global static

        if let Some(engine) = engine_opt {
            if !engine.is_null() { // Ensure the engine handle itself isn't null
                info!(
                    "[PLATFORM_MSG_CALLBACK] Sending empty response for channel: '{}'",
                    channel_name
                );
                let result = embedder::FlutterEngineSendPlatformMessageResponse(
                    engine,
                    message.response_handle,
                    ptr::null(), // No data in the response
                    0,           // Data length 0
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
                // Not sending a response here will leak the response_handle.
            }
        } else {
            error!(
                "[PLATFORM_MSG_CALLBACK] GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES is None. Cannot send response for channel: '{}'",
                channel_name
            );
            // Not sending a response here will leak the response_handle.
        }
    }
    // Any original logic for specific channels would go here, but for debugging deadlocks,
    // keeping it minimal and non-blocking is key.
}
