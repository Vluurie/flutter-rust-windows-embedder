use crate::software_renderer::dynamic_flutter_engine_dll_loader::compute_dll_search_path;
use std::path::{Path, PathBuf};

#[test]
fn search_path_uses_given_dir() {
    let dir = Path::new("C:/flutter/engine");
    let result = compute_dll_search_path(Some(dir)).unwrap();
    assert_eq!(result, PathBuf::from("C:/flutter/engine"));
}

#[test]
fn search_path_none_falls_back_to_exe_parent() {
    let result = compute_dll_search_path(None).unwrap();
    assert!(result.is_absolute());
    assert!(result.parent().is_some() || result.components().count() >= 1);
}
