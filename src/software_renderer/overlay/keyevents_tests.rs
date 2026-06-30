use winapi::um::winuser::GetKeyboardLayout;

use crate::software_renderer::overlay::keyevents::windows_to_flutter_key_codes;

const LOGICAL_ARROW_LEFT: u64 = 0x1_0000_0302;
const LOGICAL_ARROW_RIGHT: u64 = 0x1_0000_0303;
const LOGICAL_ARROW_UP: u64 = 0x1_0000_0304;
const LOGICAL_ARROW_DOWN: u64 = 0x1_0000_0301;
const LOGICAL_BACKSPACE: u64 = 0x1_0000_0008;

const PHYSICAL_ARROW_LEFT: u64 = 0x7_0050;
const PHYSICAL_ARROW_RIGHT: u64 = 0x7_004f;
const PHYSICAL_ARROW_UP: u64 = 0x7_0052;
const PHYSICAL_ARROW_DOWN: u64 = 0x7_0051;
const PHYSICAL_BACKSPACE: u64 = 0x7_002a;

const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_BACK: u16 = 0x08;

const SCAN_LEFT: u32 = 0x4b;
const SCAN_UP: u32 = 0x48;
const SCAN_RIGHT: u32 = 0x4d;
const SCAN_DOWN: u32 = 0x50;
const SCAN_BACK: u32 = 0x0e;

fn codes(vk: u16, scan: u32, extended: bool) -> (u64, u64) {
    let hkl = unsafe { GetKeyboardLayout(0) };
    windows_to_flutter_key_codes(vk, scan, extended, hkl)
}

#[test]
fn arrow_left_matches_flutter() {
    let (physical, logical) = codes(VK_LEFT, SCAN_LEFT, true);
    assert_eq!(logical, LOGICAL_ARROW_LEFT, "arrowLeft logical");
    assert_eq!(physical, PHYSICAL_ARROW_LEFT, "arrowLeft physical");
}

#[test]
fn arrow_right_matches_flutter() {
    let (physical, logical) = codes(VK_RIGHT, SCAN_RIGHT, true);
    assert_eq!(logical, LOGICAL_ARROW_RIGHT, "arrowRight logical");
    assert_eq!(physical, PHYSICAL_ARROW_RIGHT, "arrowRight physical");
}

#[test]
fn arrow_up_matches_flutter() {
    let (physical, logical) = codes(VK_UP, SCAN_UP, true);
    assert_eq!(logical, LOGICAL_ARROW_UP, "arrowUp logical");
    assert_eq!(physical, PHYSICAL_ARROW_UP, "arrowUp physical");
}

#[test]
fn arrow_down_matches_flutter() {
    let (physical, logical) = codes(VK_DOWN, SCAN_DOWN, true);
    assert_eq!(logical, LOGICAL_ARROW_DOWN, "arrowDown logical");
    assert_eq!(physical, PHYSICAL_ARROW_DOWN, "arrowDown physical");
}

#[test]
fn backspace_matches_flutter() {
    let (physical, logical) = codes(VK_BACK, SCAN_BACK, false);
    assert_eq!(logical, LOGICAL_BACKSPACE, "backspace logical");
    assert_eq!(physical, PHYSICAL_BACKSPACE, "backspace physical");
}

const VK_A: u16 = 0x41;
const SCAN_A: u32 = 0x1e;
const VK_1: u16 = 0x31;
const SCAN_1: u32 = 0x02;
const VK_SHIFT: u16 = 0x10;
const SCAN_SHIFT_LEFT: u32 = 0x2a;
const VK_NUMPAD4: u16 = 0x64;

const LOGICAL_A: u64 = 0x61;
const LOGICAL_1: u64 = 0x31;
const LOGICAL_SHIFT_LEFT: u64 = 0x2_0000_0102;
const LOGICAL_NUMPAD4: u64 = 0x2_0000_0234;

#[test]
fn letter_a_unaffected() {
    let (_physical, logical) = codes(VK_A, SCAN_A, false);
    assert_eq!(logical, LOGICAL_A, "letter a logical");
}

#[test]
fn digit_one_unaffected() {
    let (_physical, logical) = codes(VK_1, SCAN_1, false);
    assert_eq!(logical, LOGICAL_1, "digit 1 logical");
}

#[test]
fn shift_left_unaffected() {
    let (_physical, logical) = codes(VK_SHIFT, SCAN_SHIFT_LEFT, false);
    assert_eq!(logical, LOGICAL_SHIFT_LEFT, "shiftLeft logical");
}

#[test]
fn numpad4_not_confused_with_arrow_left() {
    let (_physical, logical) = codes(VK_NUMPAD4, SCAN_LEFT, false);
    assert_eq!(
        logical, LOGICAL_NUMPAD4,
        "numpad4 (same scancode as arrowLeft but NOT extended) must stay numpad4"
    );
    assert_ne!(logical, LOGICAL_ARROW_LEFT, "numpad4 must not map to arrowLeft");
}
