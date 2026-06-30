use crate::software_renderer::overlay::platform_message_callback::{
    FlutterChannel, mc_parse_method_call, mc_read_size,
};
use std::io::Cursor;

#[test]
fn from_str_known_channels() {
    assert_eq!(FlutterChannel::from_str("flutter/mousecursor"), FlutterChannel::MouseCursor);
    assert_eq!(FlutterChannel::from_str("flutter/textinput"), FlutterChannel::TextInput);
    assert_eq!(FlutterChannel::from_str("flutter/platform"), FlutterChannel::Platform);
    assert_eq!(FlutterChannel::from_str("flutter/keyboard"), FlutterChannel::Keyboard);
    assert_eq!(FlutterChannel::from_str("flutter/keyevent"), FlutterChannel::KeyEvent);
    assert_eq!(FlutterChannel::from_str("flutter/navigation"), FlutterChannel::Navigation);
}

#[test]
fn from_str_custom_channel() {
    assert_eq!(
        FlutterChannel::from_str("flutter_embedder/satellite_window"),
        FlutterChannel::Custom("flutter_embedder/satellite_window")
    );
}

#[test]
fn from_str_unknown_flutter_channel() {
    assert_eq!(
        FlutterChannel::from_str("flutter/somethingnew"),
        FlutterChannel::Unknown("flutter/somethingnew")
    );
}

#[test]
fn mc_read_size_small() {
    let data = [10u8];
    let mut c = Cursor::new(&data[..]);
    assert_eq!(mc_read_size(&mut c).unwrap(), 10);
}

#[test]
fn mc_read_size_u16() {
    let data = [254u8, 0x02, 0x01];
    let mut c = Cursor::new(&data[..]);
    assert_eq!(mc_read_size(&mut c).unwrap(), 258);
}

#[test]
fn mc_read_size_u32() {
    let data = [255u8, 0x00, 0x00, 0x01, 0x00];
    let mut c = Cursor::new(&data[..]);
    assert_eq!(mc_read_size(&mut c).unwrap(), 65536);
}

#[test]
fn mc_parse_method_call_no_args() {
    let mut data = vec![12u8, 1, 7, 4];
    data.extend_from_slice(b"test");
    let mut c = Cursor::new(&data[..]);
    let (method, kind) = mc_parse_method_call(&mut c).unwrap();
    assert_eq!(method, "test");
    assert_eq!(kind, None);
}

#[test]
fn mc_parse_method_call_with_kind_arg() {
    let method_name = b"activateSystemCursor";
    let mut data = vec![12u8, 2, 7, method_name.len() as u8];
    data.extend_from_slice(method_name);
    data.push(13);
    data.push(1);
    data.push(7);
    data.push(4);
    data.extend_from_slice(b"kind");
    data.push(7);
    data.push(4);
    data.extend_from_slice(b"text");
    let mut c = Cursor::new(&data[..]);
    let (method, kind) = mc_parse_method_call(&mut c).unwrap();
    assert_eq!(method, "activateSystemCursor");
    assert_eq!(kind, Some("text".to_string()));
}
