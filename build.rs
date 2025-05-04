use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let artifacts_dir = manifest_dir.join("flutter_artifacts");
    let include_dir = artifacts_dir.join("include");
    let lib_dir = artifacts_dir.join("lib");

    assert!(artifacts_dir.is_dir(), "flutter_artifacts/ not found");
    assert!(include_dir.is_dir(), "flutter_artifacts/include/ not found");
    assert!(lib_dir.is_dir(), "flutter_artifacts/lib/ not found");
    assert!(
        lib_dir.join("flutter_windows.lib").is_file(),
        "missing flutter_windows.lib"
    );
    assert!(
        lib_dir.join("flutter_engine.lib").is_file(),
        "missing flutter_engine.lib"
    );

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=flutter_windows");
    println!("cargo:rustc-link-lib=dylib=flutter_engine");

    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("flutter_windows.h").display()
    );
    println!("cargo:rerun-if-changed={}", include_dir.display());

    let header = include_dir.join("flutter_windows.h");
    let header_str = header.to_str().unwrap();
    let bindings = bindgen::Builder::default()
        .header(header_str)
        .clang_arg(format!("-I{}", include_dir.display()))
           .allowlist_type("FlutterDesktopEngineRef")
           .allowlist_type("FlutterDesktopPluginRegistrarRef")
           .allowlist_type("FlutterDesktopViewControllerRef")
           .allowlist_type("FlutterDesktopViewRef")
           .allowlist_type("FlutterDesktopEngineProperties")
           .allowlist_type("HWND")
           .allowlist_type("WPARAM")
           .allowlist_type("LPARAM")
           .allowlist_type("LRESULT")
           .allowlist_type("UINT")
   
           .allowlist_function("FlutterDesktopEngineCreate")
           .allowlist_function("FlutterDesktopEngineDestroy")
           .allowlist_function("FlutterDesktopEngineGetPluginRegistrar")
           .allowlist_function("FlutterDesktopEngineProcessExternalWindowMessage")
           .allowlist_function("FlutterDesktopViewControllerCreate")
           .allowlist_function("FlutterDesktopViewControllerGetView")
           .allowlist_function("FlutterDesktopViewControllerGetEngine")
           .allowlist_function("FlutterDesktopViewControllerHandleTopLevelWindowProc")
           .allowlist_function("FlutterDesktopViewControllerDestroy")
           .allowlist_function("FlutterDesktopViewGetHWND")
        .generate()
        .expect("Unable to generate bindings");
    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("Couldn't write bindings");
}
