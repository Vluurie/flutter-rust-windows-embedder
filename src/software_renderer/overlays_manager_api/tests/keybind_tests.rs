use crate::software_renderer::overlays_manager_api::keybind::parse_keybind;

#[test]
fn parse_function_key() {
    let kb = parse_keybind("F5").unwrap();
    assert_eq!(kb.vk, 0x74);
    assert!(!kb.ctrl && !kb.shift && !kb.alt);
}

#[test]
fn parse_function_key_range() {
    assert_eq!(parse_keybind("F1").unwrap().vk, 0x70);
    assert_eq!(parse_keybind("F12").unwrap().vk, 0x7B);
    assert_eq!(parse_keybind("F24").unwrap().vk, 0x87);
}

#[test]
fn parse_arrow_keys() {
    assert_eq!(parse_keybind("UP").unwrap().vk, 0x26);
    assert_eq!(parse_keybind("DOWN").unwrap().vk, 0x28);
    assert_eq!(parse_keybind("LEFT").unwrap().vk, 0x25);
    assert_eq!(parse_keybind("RIGHT").unwrap().vk, 0x27);
}

#[test]
fn parse_common_keys() {
    assert_eq!(parse_keybind("ESCAPE").unwrap().vk, 0x1B);
    assert_eq!(parse_keybind("ESC").unwrap().vk, 0x1B);
    assert_eq!(parse_keybind("ENTER").unwrap().vk, 0x0D);
    assert_eq!(parse_keybind("SPACE").unwrap().vk, 0x20);
}

#[test]
fn parse_single_letter_and_digit() {
    assert_eq!(parse_keybind("A").unwrap().vk, b'A' as u16);
    assert_eq!(parse_keybind("Z").unwrap().vk, b'Z' as u16);
    assert_eq!(parse_keybind("0").unwrap().vk, b'0' as u16);
    assert_eq!(parse_keybind("9").unwrap().vk, b'9' as u16);
}

#[test]
fn parse_with_modifiers() {
    let kb = parse_keybind("Ctrl+Shift+F5").unwrap();
    assert_eq!(kb.vk, 0x74);
    assert!(kb.ctrl && kb.shift && !kb.alt);
}

#[test]
fn parse_all_modifiers() {
    let kb = parse_keybind("Ctrl+Shift+Alt+A").unwrap();
    assert!(kb.ctrl && kb.shift && kb.alt);
    assert_eq!(kb.vk, b'A' as u16);
}

#[test]
fn parse_is_case_insensitive() {
    let a = parse_keybind("ctrl+SHIFT+f5").unwrap();
    let b = parse_keybind("CTRL+shift+F5").unwrap();
    assert_eq!(a.vk, b.vk);
    assert_eq!((a.ctrl, a.shift, a.alt), (b.ctrl, b.shift, b.alt));
    assert!(a.ctrl && a.shift);
}

#[test]
fn parse_control_alias() {
    let a = parse_keybind("Control+A").unwrap();
    assert!(a.ctrl);
}

#[test]
fn parse_handles_whitespace() {
    let kb = parse_keybind(" Ctrl + F5 ").unwrap();
    assert_eq!(kb.vk, 0x74);
    assert!(kb.ctrl);
}

#[test]
fn parse_invalid_primary_key_is_none() {
    assert!(parse_keybind("Ctrl+UNKNOWNKEY").is_none());
    assert!(parse_keybind("XYZ").is_none());
}

#[test]
fn parse_multiple_primary_keys_is_none() {
    assert!(parse_keybind("F1+F2").is_none());
    assert!(parse_keybind("A+B").is_none());
}

#[test]
fn parse_modifier_only_is_none() {
    assert!(parse_keybind("Ctrl").is_none());
    assert!(parse_keybind("Ctrl+Shift").is_none());
}

#[test]
fn parse_numpad_keys() {
    assert_eq!(parse_keybind("NUMPAD4").unwrap().vk, 0x64);
    assert_eq!(parse_keybind("NUM0").unwrap().vk, 0x60);
}
