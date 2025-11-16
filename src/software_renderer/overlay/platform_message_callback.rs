use crate::bindings::embedder::{self};
use crate::software_renderer::api::FlutterEmbedderError;
use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;
use crate::software_renderer::overlay::textinput::custom_text_input_platform_message_handler;

use byteorder::{LittleEndian, ReadBytesExt};
use log::error;
use serde_json::json;
use std::ffi::{CStr, CString, c_void};
use std::io::{Cursor, Error as IoError, ErrorKind as IoErrorKind, Read};
use std::sync::Arc;
use std::{ptr, str};
use winapi::shared::minwindef::HGLOBAL;
use winapi::um::winbase::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use winapi::um::winuser::{CF_UNICODETEXT, GetAsyncKeyState};
use winapi::um::winuser::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

/// Enum representing all known Flutter platform channels
#[derive(Debug, PartialEq, Eq)]
enum FlutterChannel<'a> {
    /// Mouse cursor channel - handles cursor appearance changes
    MouseCursor,
    /// Text input channel - handles text input state and methods
    TextInput,
    /// Accessibility channel - handles accessibility features
    Accessibility,
    /// Platform channel - handles general platform methods
    Platform,
    /// Keyboard channel - handles keyboard state queries
    Keyboard,
    /// Key event channel - handles legacy key events
    KeyEvent,
    /// Isolate channel - handles Dart isolate lifecycle events
    Isolate,
    /// Navigation channel - handles route/navigation events
    Navigation,
    /// Custom application-defined channel
    Custom(&'a str),
    /// Unknown or unrecognized channel
    Unknown(&'a str),
}

impl<'a> FlutterChannel<'a> {
    /// Parse a channel name string into a FlutterChannel enum
    fn from_str(channel: &'a str) -> Self {
        match channel {
            "flutter/mousecursor" => FlutterChannel::MouseCursor,
            "flutter/textinput" => FlutterChannel::TextInput,
            "flutter/accessibility" => FlutterChannel::Accessibility,
            "flutter/platform" => FlutterChannel::Platform,
            "flutter/keyboard" => FlutterChannel::Keyboard,
            "flutter/keyevent" => FlutterChannel::KeyEvent,
            "flutter/isolate" => FlutterChannel::Isolate,
            "flutter/navigation" => FlutterChannel::Navigation,
            _ => {
                if channel.starts_with("flutter/") {
                    FlutterChannel::Unknown(channel)
                } else {
                    FlutterChannel::Custom(channel)
                }
            }
        }
    }

    /// Get the channel name as a string
    fn as_str(&self) -> &str {
        match self {
            FlutterChannel::MouseCursor => "flutter/mousecursor",
            FlutterChannel::TextInput => "flutter/textinput",
            FlutterChannel::Accessibility => "flutter/accessibility",
            FlutterChannel::Platform => "flutter/platform",
            FlutterChannel::Keyboard => "flutter/keyboard",
            FlutterChannel::KeyEvent => "flutter/keyevent",
            FlutterChannel::Isolate => "flutter/isolate",
            FlutterChannel::Navigation => "flutter/navigation",
            FlutterChannel::Custom(name) | FlutterChannel::Unknown(name) => name,
        }
    }
}

// Standard Method Codec type tags used for parsing simple messages like mouse cursor activation.
const K_SMC_NULL: u8 = 0;
const K_SMC_TRUE: u8 = 1;
const K_SMC_FALSE: u8 = 2;
const K_SMC_INT32: u8 = 3;
const K_SMC_STRING: u8 = 7;
const K_SMC_LIST: u8 = 12;
const K_SMC_MAP: u8 = 13;

//  helper functions to decode simple messages without a full codec dependency.

fn read_exact_checked(
    cursor: &mut Cursor<&[u8]>,
    buf: &mut [u8],
    _type_name: &str,
) -> Result<(), IoError> {
    match cursor.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == IoErrorKind::UnexpectedEof => {
            Err(IoError::new(IoErrorKind::UnexpectedEof, "Unexpected EOF"))
        }
        Err(e) => Err(e),
    }
}

fn mc_read_size(cursor: &mut Cursor<&[u8]>) -> Result<usize, IoError> {
    let first_byte = cursor.read_u8()?;
    match first_byte {
        254 => Ok(cursor.read_u16::<LittleEndian>()? as usize),
        255 => Ok(cursor.read_u32::<LittleEndian>()? as usize),
        size => Ok(size as usize),
    }
}

fn mc_read_string(cursor: &mut Cursor<&[u8]>) -> Result<String, IoError> {
    let len = mc_read_size(cursor)?;
    let mut buffer = vec![0; len];
    read_exact_checked(cursor, &mut buffer, "string data")?;
    String::from_utf8(buffer).map_err(|e| IoError::new(IoErrorKind::InvalidData, e))
}

fn mc_read_cursor_kind_value(cursor: &mut Cursor<&[u8]>) -> Result<Option<String>, IoError> {
    let type_tag = cursor.read_u8()?;
    match type_tag {
        K_SMC_STRING => Ok(Some(mc_read_string(cursor)?)),
        K_SMC_NULL | K_SMC_TRUE | K_SMC_FALSE => Ok(None),
        K_SMC_INT32 => {
            let _ = cursor.read_i32::<LittleEndian>()?;
            Ok(None)
        }
        _ => Err(IoError::new(
            IoErrorKind::InvalidData,
            "Unsupported type tag",
        )),
    }
}

fn mc_parse_args_map(cursor: &mut Cursor<&[u8]>) -> Result<Option<String>, IoError> {
    if cursor.read_u8()? != K_SMC_MAP {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected K_MAP for mc args",
        ));
    }
    let map_size = mc_read_size(cursor)?;
    let mut kind_value: Option<String> = None;
    for _ in 0..map_size {
        if cursor.read_u8()? != K_SMC_STRING {
            return Err(IoError::new(
                IoErrorKind::InvalidData,
                "Expected K_STRING for mc arg key",
            ));
        }
        let key = mc_read_string(cursor)?;
        if key == "kind" {
            kind_value = mc_read_cursor_kind_value(cursor)?;
        } else {
            let _ = mc_read_cursor_kind_value(cursor);
        }
    }
    Ok(kind_value)
}

fn mc_parse_method_call(cursor: &mut Cursor<&[u8]>) -> Result<(String, Option<String>), IoError> {
    if cursor.read_u8()? != K_SMC_LIST {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected K_LIST for mc call",
        ));
    }
    let list_size = mc_read_size(cursor)?;

    if cursor.read_u8()? != K_SMC_STRING {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected K_STRING for mc method name",
        ));
    }
    let method_name = mc_read_string(cursor)?;

    let args_kind_value: Option<String>;
    if list_size > 1 && cursor.position() < cursor.get_ref().len() as u64 {
        let args_tag = cursor.read_u8()?;
        match args_tag {
            K_SMC_NULL => args_kind_value = None,
            K_SMC_MAP => {
                cursor.set_position(cursor.position() - 1);
                args_kind_value = mc_parse_args_map(cursor)?;
            }
            _ => {
                args_kind_value = None;
            }
        }
    } else {
        args_kind_value = None;
    }
    Ok((method_name, args_kind_value))
}

/// Result type for channel handlers
enum ChannelHandlerResult {
    /// Response was sent, no further action needed
    Handled,
    /// Response needs to be sent with provided data
    RespondWith(Vec<u8>),
    /// Send a null/empty response
    RespondNull,
    /// No response needed or already handled elsewhere
    NoResponse,
}

/// Handle messages on the flutter/mousecursor channel
fn handle_mousecursor_message(
    message: &embedder::FlutterPlatformMessage,
    overlay: &FlutterOverlay,
) -> ChannelHandlerResult {
    unsafe {
        if message.message_size > 0 && !message.message.is_null() {
            let slice = std::slice::from_raw_parts(message.message, message.message_size);
            let mut msg_cursor = Cursor::new(slice);
            if !slice.is_empty() {
                match slice[0] {
                    K_SMC_LIST => {
                        if let Ok((method, kind_opt)) = mc_parse_method_call(&mut msg_cursor) {
                            if method == "activateSystemCursor" {
                                if let Ok(mut guard) = overlay.desired_cursor.lock() {
                                    *guard = kind_opt;
                                }
                            }
                        }
                    }
                    K_SMC_STRING | K_SMC_NULL | K_SMC_INT32 | K_SMC_TRUE | K_SMC_FALSE => {
                        if let Ok(kind_opt) = mc_read_cursor_kind_value(&mut msg_cursor) {
                            if let Ok(mut guard) = overlay.desired_cursor.lock() {
                                *guard = kind_opt;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    ChannelHandlerResult::RespondNull
}

/// Handle messages on the flutter/keyboard channel
fn handle_keyboard_message(message: &embedder::FlutterPlatformMessage) -> ChannelHandlerResult {
    unsafe {
        let slice = std::slice::from_raw_parts(message.message, message.message_size);

        if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(slice) {
            if let Some(method) = json_value.get("method").and_then(|m| m.as_str()) {
                match method {
                    "Keyboard.getState" => {
                        let mut flutter_modifiers = 0;
                        if (GetAsyncKeyState(VK_SHIFT) & 0x8000u16 as i16) != 0 {
                            flutter_modifiers |= 0x1; // Shift
                        }
                        if (GetAsyncKeyState(VK_CONTROL) & 0x8000u16 as i16) != 0 {
                            flutter_modifiers |= 0x2; // Control
                        }
                        if (GetAsyncKeyState(VK_MENU) & 0x8000u16 as i16) != 0 {
                            flutter_modifiers |= 0x4; // Alt
                        }
                        if ((GetAsyncKeyState(VK_LWIN) & 0x8000u16 as i16) != 0)
                            || ((GetAsyncKeyState(VK_RWIN) & 0x8000u16 as i16) != 0)
                        {
                            flutter_modifiers |= 0x8; // Meta (Windows Key)
                        }

                        let response_payload = json!({ "modifiers": flutter_modifiers });
                        let response_bytes = response_payload.to_string().into_bytes();

                        return ChannelHandlerResult::RespondWith(response_bytes);
                    }
                    _ => {
                        // Unknown keyboard method - send null response
                    }
                }
            }
        }
    }
    ChannelHandlerResult::RespondNull
}

/// Handle messages on the flutter/isolate channel
fn handle_isolate_message(_message: &embedder::FlutterPlatformMessage) -> ChannelHandlerResult {
    // Isolate messages are typically lifecycle notifications from Dart
    // We acknowledge them but don't need to process them
    ChannelHandlerResult::RespondNull
}

/// Handle messages on the flutter/navigation channel
fn handle_navigation_message(_message: &embedder::FlutterPlatformMessage) -> ChannelHandlerResult {
    // Navigation messages are route push/pop notifications
    // Acknowledge without processing
    ChannelHandlerResult::RespondNull
}

/// Handle messages on the flutter/platform channel (clipboard, system chrome, etc.)
fn handle_platform_message(message: &embedder::FlutterPlatformMessage) -> ChannelHandlerResult {
    unsafe {
        let slice = std::slice::from_raw_parts(message.message, message.message_size);

        if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(slice) {
            if let Some(method) = json_value.get("method").and_then(|m| m.as_str()) {
                match method {
                    "Clipboard.getData" => {
                        if let Some(text) = get_clipboard_text() {
                            let response = json!([{"text": text}]);
                            return ChannelHandlerResult::RespondWith(
                                response.to_string().into_bytes(),
                            );
                        }
                        let response = json!([null]);
                        ChannelHandlerResult::RespondWith(response.to_string().into_bytes())
                    }
                    "Clipboard.setData" => {
                        if let Some(args) = json_value.get("args") {
                            if let Some(text) = args.get("text").and_then(|t| t.as_str()) {
                                set_clipboard_text(text);
                            }
                        }
                        let response = json!([null]);
                        ChannelHandlerResult::RespondWith(response.to_string().into_bytes())
                    }
                    "Clipboard.hasStrings" => {
                        let has_text = has_clipboard_text();
                        let response = json!([{"value": has_text}]);
                        ChannelHandlerResult::RespondWith(response.to_string().into_bytes())
                    }
                    _ => {
                        // Unknown platform method - acknowledge with null envelope to prevent errors
                        // This allows Flutter widgets to call any platform method without crashing
                        let response = json!([null]);
                        ChannelHandlerResult::RespondWith(response.to_string().into_bytes())
                    }
                }
            } else {
                ChannelHandlerResult::RespondNull
            }
        } else {
            ChannelHandlerResult::RespondNull
        }
    }
}

/// Get text from Windows clipboard
fn get_clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }

        let result = if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 {
            let h_data = GetClipboardData(CF_UNICODETEXT);
            if !h_data.is_null() {
                let p_data = GlobalLock(h_data) as *const u16;
                if !p_data.is_null() {
                    let size = GlobalSize(h_data) / 2;
                    let mut len = 0;
                    while len < size && *p_data.offset(len as isize) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(p_data, len);
                    let text = String::from_utf16_lossy(slice);
                    GlobalUnlock(h_data);
                    Some(text)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        CloseClipboard();
        result
    }
}

/// Set text to Windows clipboard
fn set_clipboard_text(text: &str) {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return;
        }

        EmptyClipboard();

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;

        let h_mem = GlobalAlloc(GMEM_MOVEABLE, size);
        if !h_mem.is_null() {
            let p_mem = GlobalLock(h_mem) as *mut u16;
            if !p_mem.is_null() {
                ptr::copy_nonoverlapping(wide.as_ptr(), p_mem, wide.len());
                GlobalUnlock(h_mem);
                SetClipboardData(CF_UNICODETEXT, h_mem as HGLOBAL);
            }
        }

        CloseClipboard();
    }
}

/// Check if Windows clipboard has text
fn has_clipboard_text() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 }
}

/// Handle custom application-defined channels
fn handle_custom_channel(
    channel_name: &str,
    message: &embedder::FlutterPlatformMessage,
    overlay: &FlutterOverlay,
) -> ChannelHandlerResult {
    unsafe {
        let data_slice = std::slice::from_raw_parts(message.message, message.message_size);

        match overlay.message_handlers.lock() {
            Ok(handlers) => {
                if let Some(handler) = handlers.get(channel_name) {
                    let request_payload = data_slice.to_vec();
                    let response_payload = handler(request_payload);
                    ChannelHandlerResult::RespondWith(response_payload)
                } else {
                    ChannelHandlerResult::RespondNull
                }
            }
            Err(poisoned) => {
                error!(
                    "[PlatformMsgCB] Handlers map mutex poisoned for channel '{}': {}",
                    channel_name, poisoned
                );
                ChannelHandlerResult::RespondNull
            }
        }
    }
}

/// Send a response back to Flutter for a platform message
fn send_response(
    engine_handle: embedder::FlutterEngine,
    engine_dll: &Arc<FlutterEngineDll>,
    response_handle: *const embedder::FlutterPlatformMessageResponseHandle,
    data: Option<&[u8]>,
) {
    if response_handle.is_null() {
        return;
    }

    let (ptr, len) = match data {
        Some(bytes) => (bytes.as_ptr(), bytes.len()),
        None => (ptr::null(), 0),
    };

    unsafe {
        let result = (engine_dll.FlutterEngineSendPlatformMessageResponse)(
            engine_handle,
            response_handle,
            ptr,
            len,
        );

        if result != embedder::FlutterEngineResult_kSuccess {
            error!("[PlatformMsgCB] Failed to send response: {:?}", result);
        }
    }
}

pub(crate) extern "C" fn simple_platform_message_callback(
    platform_message: *const embedder::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    unsafe {
        // Validate inputs
        if platform_message.is_null() {
            error!("[PlatformMsgCB] Received null platform_message pointer");
            return;
        }

        if user_data.is_null() {
            error!("[PlatformMsgCB] user_data is null. Cannot process message.");
            return;
        }

        let overlay: &mut FlutterOverlay = &mut *(user_data as *mut FlutterOverlay);
        let engine_handle = overlay.engine;
        let engine_dll_arc = overlay.engine_dll.clone();
        let message = &*platform_message;

        // Handle messages with no channel name
        if message.channel.is_null() {
            send_response(
                engine_handle.0,
                &engine_dll_arc,
                message.response_handle,
                None,
            );
            return;
        }

        // Parse channel name
        let channel_name_c_str = CStr::from_ptr(message.channel);
        let channel_name_str = channel_name_c_str.to_string_lossy();
        let channel_name = channel_name_str.as_ref();
        let channel = FlutterChannel::from_str(channel_name);

        // Route message to appropriate handler based on channel
        let result = match channel {
            FlutterChannel::MouseCursor => handle_mousecursor_message(message, overlay),

            FlutterChannel::TextInput => {
                // TextInput handler sends its own response
                custom_text_input_platform_message_handler(platform_message, user_data);
                ChannelHandlerResult::Handled
            }

            FlutterChannel::Accessibility => {
                // Accessibility channel just needs acknowledgment
                ChannelHandlerResult::RespondNull
            }

            FlutterChannel::Platform => handle_platform_message(message),

            FlutterChannel::Keyboard => handle_keyboard_message(message),

            FlutterChannel::KeyEvent => {
                // Key event channel - respond null
                ChannelHandlerResult::RespondNull
            }

            FlutterChannel::Isolate => handle_isolate_message(message),

            FlutterChannel::Navigation => handle_navigation_message(message),

            FlutterChannel::Custom(name) => handle_custom_channel(name, message, overlay),

            FlutterChannel::Unknown(_name) => ChannelHandlerResult::RespondNull,
        };

        // Send response based on handler result
        match result {
            ChannelHandlerResult::Handled => {
                // Response already sent by handler
            }
            ChannelHandlerResult::RespondWith(data) => {
                send_response(
                    engine_handle.0,
                    &engine_dll_arc,
                    message.response_handle,
                    Some(&data),
                );
            }
            ChannelHandlerResult::RespondNull => {
                send_response(
                    engine_handle.0,
                    &engine_dll_arc,
                    message.response_handle,
                    None,
                );
            }
            ChannelHandlerResult::NoResponse => {
                // No response needed
            }
        }
    }
}

/// Sends a platform message from Rust to the Dart application.
///
/// # Arguments
/// * `overlay`: A reference to the active `FlutterOverlay`.
/// * `channel`: The name of the channel to send the message on.
/// * `message`: A byte slice representing the message payload.
///
/// # Returns
/// `Ok(())` on success, or a `FlutterEmbedderError` on failure.
pub fn send_platform_message(
    overlay: &FlutterOverlay,
    channel: &str,
    message: &[u8],
) -> Result<(), FlutterEmbedderError> {
    if overlay.engine.0.is_null() {
        return Err(FlutterEmbedderError::EngineNotRunning);
    }

    let channel_cstring = match CString::new(channel) {
        Ok(s) => s,
        Err(e) => {
            return Err(FlutterEmbedderError::OperationFailed(format!(
                "Invalid channel name: {}",
                e
            )));
        }
    };

    let platform_message = embedder::FlutterPlatformMessage {
        struct_size: std::mem::size_of::<embedder::FlutterPlatformMessage>(),
        channel: channel_cstring.as_ptr(),
        message: message.as_ptr(),
        message_size: message.len(),
        response_handle: ptr::null(),
    };

    let result = unsafe {
        (overlay.engine_dll.FlutterEngineSendPlatformMessage)(overlay.engine.0, &platform_message)
    };

    if result == embedder::FlutterEngineResult_kSuccess {
        Ok(())
    } else {
        let err_msg = format!(
            "Failed to send platform message on channel '{}': {:?}",
            channel, result
        );
        error!("[FlutterOverlay:'{}'] {}", overlay.name, err_msg);
        Err(FlutterEmbedderError::OperationFailed(err_msg))
    }
}
