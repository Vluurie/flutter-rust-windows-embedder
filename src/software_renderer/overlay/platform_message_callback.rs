use crate::bindings::embedder::{self};
use crate::software_renderer::api::FlutterEmbedderError;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;
use crate::software_renderer::overlay::textinput::custom_text_input_platform_message_handler;

use log::error;

use std::ffi::{c_void, CStr, CString};
use std::io::{Cursor, Error as IoError, ErrorKind as IoErrorKind, Read};
use std::{ptr, str};

use byteorder::{LittleEndian, ReadBytesExt};

const K_SMC_NULL: u8 = 0;
const K_SMC_TRUE: u8 = 1;
const K_SMC_FALSE: u8 = 2;
const K_SMC_INT32: u8 = 3;
const K_SMC_STRING: u8 = 7;
const K_SMC_LIST: u8 = 12;
const K_SMC_MAP: u8 = 13;

fn read_exact_checked(
    cursor: &mut Cursor<&[u8]>,
    buf: &mut [u8],
    _type_name: &str,
) -> Result<(), IoError> {
    match cursor.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == IoErrorKind::UnexpectedEof => Err(IoError::new(
            IoErrorKind::UnexpectedEof,
            format!("Unexpected EOF"),
        )),
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
            format!("Unsupported type tag"),
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

#[unsafe(no_mangle)]
pub(crate) extern "C" fn simple_platform_message_callback(
    platform_message: *const embedder::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    unsafe {
        if platform_message.is_null() {
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

        if message.channel.is_null() {
            if !message.response_handle.is_null() {
                let _ = (engine_dll_arc.FlutterEngineSendPlatformMessageResponse)(
                    engine_handle.0,
                    message.response_handle,
                    ptr::null(),
                    0,
                );
            }
            return;
        }

        let channel_name_c_str = CStr::from_ptr(message.channel);
        let channel_name_str = channel_name_c_str.to_string_lossy();
        let channel_name = channel_name_str.as_ref();

        let mut response_sent_by_handler = false;

        if channel_name == "flutter/mousecursor" {
            if message.message_size > 0 && !message.message.is_null() {
                let slice = std::slice::from_raw_parts(message.message, message.message_size);
                let mut msg_cursor = Cursor::new(slice);
                if !slice.is_empty() {
                    match slice[0] {
                        K_SMC_LIST => {
                            if let Ok((method, kind_opt)) = mc_parse_method_call(&mut msg_cursor) {
                                if method == "activateSystemCursor" {
                                    // set_desired_cursor(kind_opt);
                                    if let Ok(mut guard) = overlay.desired_cursor.lock() {
                                        *guard = kind_opt;
                                    }
                                }
                            }
                        }
                        K_SMC_STRING | K_SMC_NULL | K_SMC_INT32 | K_SMC_TRUE | K_SMC_FALSE => {
                            if let Ok(kind_opt) = mc_read_cursor_kind_value(&mut msg_cursor) {
                                // set_desired_cursor(kind_opt);
                                if let Ok(mut guard) = overlay.desired_cursor.lock() {
                                    *guard = kind_opt;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else if channel_name == "flutter/textinput" {
            custom_text_input_platform_message_handler(platform_message, user_data);
            if !message.response_handle.is_null() {
                response_sent_by_handler = true;
            }
        } else if channel_name == "flutter/accessibility" || channel_name == "flutter/platform" {
            if !message.response_handle.is_null() {
                let _ = (engine_dll_arc.FlutterEngineSendPlatformMessageResponse)(
                    engine_handle.0,
                    message.response_handle,
                    ptr::null(),
                    0,
                );
                response_sent_by_handler = true;
            }
        }

        if !response_sent_by_handler && !message.response_handle.is_null() {
            let _ = (engine_dll_arc.FlutterEngineSendPlatformMessageResponse)(
                engine_handle.0,
                message.response_handle,
                ptr::null(),
                0,
            );
        }
    }
}


/// Sends a platform message directly to the Dart side of the overlay.
/// This is the primary way to communicate with your Flutter app from Rust.
///
/// # Arguments
/// * `channel`: The name of the channel to send the message on (e.g., "app/lifecycle").
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
        Err(e) => return Err(FlutterEmbedderError::OperationFailed(format!("Invalid channel name: {}", e))),
    };

    let platform_message = embedder::FlutterPlatformMessage {
        struct_size: std::mem::size_of::<embedder::FlutterPlatformMessage>(),
        channel: channel_cstring.as_ptr(),
        message: message.as_ptr(),
        message_size: message.len(),
        response_handle: ptr::null(),
    };

    unsafe {
        let result = (overlay.engine_dll.FlutterEngineSendPlatformMessage)(overlay.engine.0, &platform_message);
        
        if result == embedder::FlutterEngineResult_kSuccess {
            Ok(())
        } else {
            let err_msg = format!("Failed to send platform message on channel '{}': {:?}", channel, result);
            error!("[FlutterOverlay:'{}'] {}", overlay.name, err_msg);
            Err(FlutterEmbedderError::OperationFailed(err_msg))
        }
    }
}