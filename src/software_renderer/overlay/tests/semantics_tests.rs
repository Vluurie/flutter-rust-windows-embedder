use crate::bindings::embedder::{
    FlutterSemanticsFlag_kFlutterSemanticsFlagHasCheckedState,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsButton,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsChecked,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsTextField,
};
use crate::software_renderer::overlay::semantics_handler::{
    RustSemanticsFlag, ffi_flags_to_rust_set,
};

#[test]
fn empty_flags_empty_set() {
    let set = ffi_flags_to_rust_set(0);
    assert!(set.is_empty());
}

#[test]
fn single_flag() {
    let set = ffi_flags_to_rust_set(FlutterSemanticsFlag_kFlutterSemanticsFlagIsButton);
    assert!(set.contains(&RustSemanticsFlag::IsButton));
    assert_eq!(set.len(), 1);
}

#[test]
fn combined_flags() {
    let flags = FlutterSemanticsFlag_kFlutterSemanticsFlagHasCheckedState
        | FlutterSemanticsFlag_kFlutterSemanticsFlagIsChecked;
    let set = ffi_flags_to_rust_set(flags);
    assert!(set.contains(&RustSemanticsFlag::HasCheckedState));
    assert!(set.contains(&RustSemanticsFlag::IsChecked));
    assert_eq!(set.len(), 2);
}

#[test]
fn textfield_flag() {
    let set = ffi_flags_to_rust_set(FlutterSemanticsFlag_kFlutterSemanticsFlagIsTextField);
    assert!(set.contains(&RustSemanticsFlag::IsTextField));
    assert!(!set.contains(&RustSemanticsFlag::IsButton));
}
