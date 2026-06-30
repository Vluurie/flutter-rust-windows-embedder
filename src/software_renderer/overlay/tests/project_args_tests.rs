use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, maybe_load_aot_path_to_cstring,
};
use std::ffi::{CString, OsStr};

#[test]
fn builds_assets_and_icu() {
    let (assets, icu, engine_argv, dart_argv) =
        build_project_args_and_strings("/path/assets", "/path/icu.dat", None, None);
    assert_eq!(assets, CString::new("/path/assets").unwrap());
    assert_eq!(icu, CString::new("/path/icu.dat").unwrap());
    assert!(engine_argv.is_empty());
    assert!(dart_argv.is_empty());
}

#[test]
fn builds_engine_and_dart_args() {
    let dart = vec!["--observe=123".to_string()];
    let engine = vec!["--disable-vsync".to_string(), "--trace".to_string()];
    let (_assets, _icu, engine_argv, dart_argv) = build_project_args_and_strings(
        "/a",
        "/b",
        Some(&dart),
        Some(&engine),
    );
    assert_eq!(engine_argv.len(), 2);
    assert_eq!(engine_argv[0], CString::new("--disable-vsync").unwrap());
    assert_eq!(engine_argv[1], CString::new("--trace").unwrap());
    assert_eq!(dart_argv.len(), 1);
    assert_eq!(dart_argv[0], CString::new("--observe=123").unwrap());
}

#[test]
fn aot_path_none() {
    assert!(maybe_load_aot_path_to_cstring(None).is_none());
}

#[test]
fn aot_path_some() {
    let p = OsStr::new("/path/app.so");
    let result = maybe_load_aot_path_to_cstring(Some(p)).unwrap();
    assert_eq!(result, CString::new("/path/app.so").unwrap());
}
