use crate::embedder;
use log::{error, info, warn};
use once_cell::sync::Lazy;

use std::ffi::{CStr, c_void};
use std::io::{Cursor, Error as IoError, ErrorKind as IoErrorKind, Read};
use std::{ptr, str};

use byteorder::{LittleEndian, ReadBytesExt};
use std::sync::Mutex;

pub static DESIRED_FLUTTER_CURSOR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

static mut GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES: Option<embedder::FlutterEngine> = None;

#[allow(dead_code)]
pub unsafe fn set_global_engine_for_platform_messages(engine: embedder::FlutterEngine) {
    unsafe { GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES = Some(engine) };
}

const K_NULL: u8 = 0;
const K_INT32: u8 = 3;
const K_STRING: u8 = 7;
const K_LIST: u8 = 12;
const K_MAP: u8 = 13;

fn read_size(cursor: &mut Cursor<&[u8]>) -> Result<usize, IoError> {
    let first_byte = cursor.read_u8()?;
    match first_byte {
        254 => Ok(cursor.read_u16::<LittleEndian>()? as usize),
        255 => Ok(cursor.read_u32::<LittleEndian>()? as usize),
        size => Ok(size as usize),
    }
}

fn read_string(cursor: &mut Cursor<&[u8]>) -> Result<String, IoError> {
    let len = read_size(cursor)?;
    let mut buffer = vec![0; len];
    cursor.read_exact(&mut buffer)?;
    String::from_utf8(buffer).map_err(|e| IoError::new(IoErrorKind::InvalidData, e))
}

fn read_value(cursor: &mut Cursor<&[u8]>) -> Result<Option<String>, IoError> {
    let type_tag = cursor.read_u8()?;
    match type_tag {
        K_STRING => Ok(Some(read_string(cursor)?)),
        K_NULL => Ok(None),
        K_INT32 => {
            let _ = cursor.read_i32::<LittleEndian>()?;
            Ok(None)
        }
        _ => Err(IoError::new(
            IoErrorKind::InvalidData,
            format!("Unsupported or unexpected value type tag: {}", type_tag),
        )),
    }
}

fn parse_mouse_cursor_args(cursor: &mut Cursor<&[u8]>) -> Result<Option<String>, IoError> {
    let type_tag = cursor.read_u8()?;
    if type_tag != K_MAP {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected map tag for arguments",
        ));
    }

    let map_size = read_size(cursor)?;
    let mut kind_value: Option<String> = None;

    for _ in 0..map_size {
        let key_type_tag = cursor.read_u8()?;
        if key_type_tag != K_STRING {
            return Err(IoError::new(
                IoErrorKind::InvalidData,
                "Expected string tag for map key",
            ));
        }
        let key = read_string(cursor)?;

        if key == "kind" {
            kind_value = read_value(cursor)?;
        } else if key == "device" {
            let _ = read_value(cursor)?;
        } else {
            return Err(IoError::new(
                IoErrorKind::InvalidData,
                format!("Unknown key in mouse cursor args map: {}", key),
            ));
        }
    }
    Ok(kind_value)
}

fn parse_method_call(message_slice: &[u8]) -> Result<(String, Option<String>), IoError> {
    let mut cursor = Cursor::new(message_slice);

    let envelope_type = cursor.read_u8()?;
    if envelope_type != K_LIST {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected List envelope for method call",
        ));
    }

    let list_size = read_size(&mut cursor)?;
    if list_size != 2 {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Method call list should have 2 elements",
        ));
    }

    let method_name_type = cursor.read_u8()?;
    if method_name_type != K_STRING {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected string for method name",
        ));
    }
    let method_name = read_string(&mut cursor)?;

    let args_kind_value = parse_mouse_cursor_args(&mut cursor)?;

    Ok((method_name, args_kind_value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_platform_message_callback(
    platform_message: *const embedder::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    unsafe {
        if platform_message.is_null() {
            error!("[PLATFORM_MSG_CALLBACK] Received null platform_message pointer.");
            return;
        }

        let message = &*platform_message;
        let channel_name_ptr = message.channel;

        if channel_name_ptr.is_null() {
            if !message.response_handle.is_null() {
                warn!(
                    "[PLATFORM_MSG_CALLBACK] Message has null channel but a response handle. Sending empty response."
                );
                if let Some(engine) = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES {
                    if !engine.is_null() {
                        embedder::FlutterEngineSendPlatformMessageResponse(
                            engine,
                            message.response_handle,
                            ptr::null(),
                            0,
                        );
                    }
                }
            }
            error!("[PLATFORM_MSG_CALLBACK] Received message with null channel name.");
            return;
        }

        let channel_name_c_str = CStr::from_ptr(channel_name_ptr);
        let channel_name = channel_name_c_str.to_string_lossy();

        if channel_name == "flutter/mousecursor" {
            if message.message_size > 0 && !message.message.is_null() {
                let message_content_slice =
                    std::slice::from_raw_parts(message.message, message.message_size);

                match parse_method_call(message_content_slice) {
                    Ok((method_name, Some(kind))) => {
                        if method_name == "activateSystemCursor" {
                            info!(
                                "[PLATFORM_MSG_CALLBACK] Deterministically parsed flutter/mousecursor: method='{}', kind='{}'",
                                method_name, kind
                            );
                            if let Ok(mut desired_cursor_guard) = DESIRED_FLUTTER_CURSOR.lock() {
                                *desired_cursor_guard = Some(kind);
                            } else {
                                error!(
                                    "[PLATFORM_MSG_CALLBACK] Failed to lock DESIRED_FLUTTER_CURSOR for kind."
                                );
                            }
                        } else {
                            warn!(
                                "[PLATFORM_MSG_CALLBACK] flutter/mousecursor: Received method call structure but unknown method: {}",
                                method_name
                            );
                        }
                    }
                    Ok((method_name, None)) => {
                        warn!(
                            "[PLATFORM_MSG_CALLBACK] flutter/mousecursor: Parsed method '{}' but 'kind' argument was null or not a string.",
                            method_name
                        );
                        if let Ok(mut desired_cursor_guard) = DESIRED_FLUTTER_CURSOR.lock() {
                            *desired_cursor_guard = None;
                        }
                    }
                    Err(e) => {
                        error!(
                            "[PLATFORM_MSG_CALLBACK] Failed to deterministically parse flutter/mousecursor message: {:?}. Slice len: {}",
                            e,
                            message_content_slice.len()
                        );
                        let max_bytes_to_log = 32.min(message_content_slice.len());
                        info!(
                            "[PLATFORM_MSG_CALLBACK] Raw bytes (first {}): {:?}",
                            max_bytes_to_log,
                            &message_content_slice[..max_bytes_to_log]
                        );
                        if let Ok(mut desired_cursor_guard) = DESIRED_FLUTTER_CURSOR.lock() {
                            *desired_cursor_guard = None;
                        }
                    }
                }
            } else {
                info!("[PLATFORM_MSG_CALLBACK] flutter/mousecursor message is empty or null.");
                if let Ok(mut desired_cursor_guard) = DESIRED_FLUTTER_CURSOR.lock() {
                    *desired_cursor_guard = None;
                }
            }

            if !message.response_handle.is_null() {
                if let Some(engine) = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES {
                    if !engine.is_null() {
                        let result = embedder::FlutterEngineSendPlatformMessageResponse(
                            engine,
                            message.response_handle,
                            ptr::null(),
                            0,
                        );
                        if result != embedder::FlutterEngineResult_kSuccess {
                            error!(
                                "[PLATFORM_MSG_CALLBACK] Failed to send response for flutter/mousecursor: {:?}",
                                result
                            );
                        }
                    }
                }
            }
            return;
        }

        if !user_data.is_null() {
            info!(
                "[PLATFORM_MSG_CALLBACK] UserData: {:?}, Channel: '{}', Size: {}",
                user_data, channel_name, message.message_size
            );
        } else {
            info!(
                "[PLATFORM_MSG_CALLBACK] UserData: null, Channel: '{}', Size: {}",
                channel_name, message.message_size
            );
        }

        if message.message_size > 0 && !message.message.is_null() {
            let message_content_slice =
                std::slice::from_raw_parts(message.message, message.message_size);
            if let Ok(message_str) = str::from_utf8(message_content_slice) {
                info!(
                    "[PLATFORM_MSG_CALLBACK] Other Channel ('{}') Content (UTF-8): {}",
                    channel_name, message_str
                );
            } else {
                let max_bytes_to_log = 64.min(message_content_slice.len());
                let bytes_to_log = &message_content_slice[..max_bytes_to_log];
                info!(
                    "[PLATFORM_MSG_CALLBACK] Other Channel ('{}') Content (raw bytes, first {} of {}): {:?}",
                    channel_name,
                    bytes_to_log.len(),
                    message_content_slice.len(),
                    bytes_to_log
                );
            }
        } else if message.message_size == 0 {
            info!(
                "[PLATFORM_MSG_CALLBACK] Other Channel ('{}'): Message content is empty.",
                channel_name
            );
        }

        if !message.response_handle.is_null() {
            if let Some(engine) = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES {
                if !engine.is_null() {
                    info!(
                        "[PLATFORM_MSG_CALLBACK] Sending default empty response for channel: '{}'",
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
                            "[PLATFORM_MSG_CALLBACK] Failed to send default response for '{}': {:?}",
                            channel_name, result
                        );
                    }
                }
            }
        }
    }
}
