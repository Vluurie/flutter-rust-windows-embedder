use crate::embedder;

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
    type_name: &str,
) -> Result<(), IoError> {
    match cursor.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == IoErrorKind::UnexpectedEof => Err(IoError::new(
            IoErrorKind::UnexpectedEof,
            format!("Unexpected EOF while reading {}", type_name),
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
    read_exact_checked(cursor, &mut buffer, "mouse cursor string")?;
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
            format!(
                "Unsupported type tag 0x{:02X} for mousecursor 'kind'",
                type_tag
            ),
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
    let mut kind_found = false;
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
            kind_found = true;
        } else if key == "device" {
            let _ = mc_read_cursor_kind_value(cursor)?;
        } else {
            let _ = mc_read_cursor_kind_value(cursor)?;
        }
    }
    if !kind_found {}
    Ok(kind_value)
}

fn mc_parse_method_call(cursor: &mut Cursor<&[u8]>) -> Result<(String, Option<String>), IoError> {
    if cursor.read_u8()? != K_SMC_LIST {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected K_LIST for mc call",
        ));
    }
    if mc_read_size(cursor)? != 2 {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "mc call list needs 2 elements",
        ));
    }
    if cursor.read_u8()? != K_SMC_STRING {
        return Err(IoError::new(
            IoErrorKind::InvalidData,
            "Expected K_STRING for mc method name",
        ));
    }
    let method_name = mc_read_string(cursor)?;
    let args_kind_value: Option<String>;
    let args_tag = cursor.read_u8()?;
    match args_tag {
        K_SMC_NULL => args_kind_value = None,
        K_SMC_MAP => {
            cursor.set_position(cursor.position() - 1);
            args_kind_value = mc_parse_args_map(cursor)?;
        }
        _ => {
            return Err(IoError::new(
                IoErrorKind::InvalidData,
                format!(
                    "Expected K_MAP or K_NULL for mc args, got 0x{:02X}",
                    args_tag
                ),
            ));
        }
    }
    Ok((method_name, args_kind_value))
}

fn set_desired_cursor(new_kind: Option<String>) {
    match DESIRED_FLUTTER_CURSOR.lock() {
        Ok(mut guard) => *guard = new_kind,
        Err(_e) => {}
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_platform_message_callback(
    platform_message: *const embedder::FlutterPlatformMessage,
    _user_data: *mut c_void,
) {
    unsafe {
    if platform_message.is_null() {
        return;
    }
    let message = &*platform_message;
    let channel_name_ptr = message.channel;

    if channel_name_ptr.is_null() {
        if !message.response_handle.is_null() {
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

        return;
    }

    let channel_name_c_str = CStr::from_ptr(channel_name_ptr);
    let channel_name_str = channel_name_c_str.to_string_lossy();
    let channel_name = channel_name_str.as_ref();
    let mut response_sent = false;

    if channel_name == "flutter/mousecursor" {
        if message.message_size > 0 && !message.message.is_null() {
            let slice = std::slice::from_raw_parts(message.message, message.message_size);
            let mut msg_cursor = Cursor::new(slice);
            if !slice.is_empty() {
                match slice[0] {
                    K_SMC_LIST => match mc_parse_method_call(&mut msg_cursor) {
                        Ok((method, kind_opt)) => {
                            if method == "activateSystemCursor" {
                                if let Some(ref k_str) = kind_opt {
                                    if k_str == "activateSystemCursor" {
                                    } else {
                                        set_desired_cursor(kind_opt);
                                    }
                                } else {
                                    set_desired_cursor(None);
                                }
                            } else {
                            }
                        }
                        Err(_e) => {}
                    },
                    K_SMC_STRING | K_SMC_NULL | K_SMC_INT32 | K_SMC_TRUE | K_SMC_FALSE => {
                        match mc_read_cursor_kind_value(&mut msg_cursor) {
                            Ok(kind_opt) => {
                                if let Some(ref k_str) = kind_opt {
                                    if k_str == "activateSystemCursor" {
                                    } else {
                                        set_desired_cursor(kind_opt);
                                    }
                                } else {
                                    set_desired_cursor(None);
                                }
                            }
                            Err(_e) => {}
                        }
                    }
                    _tag => {}
                }
            } else {
            }
        } else {
        }

        if !message.response_handle.is_null() {
            if let Some(engine) = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES {
                if !engine.is_null() {
                    if embedder::FlutterEngineSendPlatformMessageResponse(
                        engine,
                        message.response_handle,
                        ptr::null(),
                        0,
                    ) != embedder::FlutterEngineResult_kSuccess
                    {}
                    response_sent = true;
                }
            }
        }
    } else if channel_name == "flutter/accessibility" {
        if message.message_size > 0 && !message.message.is_null() {
        } else {
        }

        if !message.response_handle.is_null() {
            if let Some(engine) = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES {
                if !engine.is_null() {
                    embedder::FlutterEngineSendPlatformMessageResponse(
                        engine,
                        message.response_handle,
                        ptr::null(),
                        0,
                    );
                    response_sent = true;
                }
            }
        }
    } else if channel_name == "flutter/platform" {
        if message.message_size > 0 && !message.message.is_null() {
            let slice = std::slice::from_raw_parts(message.message, message.message_size);
            match str::from_utf8(slice) {
                Ok(_json_str) => {}
                Err(_e) => {}
            }
        } else {
        }

        if !message.response_handle.is_null() {
            if let Some(engine) = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES {
                if !engine.is_null() {
                    embedder::FlutterEngineSendPlatformMessageResponse(
                        engine,
                        message.response_handle,
                        ptr::null(),
                        0,
                    );
                    response_sent = true;
                }
            }
        }
    } else {
    }

    if !response_sent && !message.response_handle.is_null() {
        if let Some(engine) = GLOBAL_ENGINE_FOR_PLATFORM_MESSAGES {
            if !engine.is_null() {
                if embedder::FlutterEngineSendPlatformMessageResponse(
                    engine,
                    message.response_handle,
                    ptr::null(),
                    0,
                ) != embedder::FlutterEngineResult_kSuccess
                {}
            }
        }
    }
}
}
