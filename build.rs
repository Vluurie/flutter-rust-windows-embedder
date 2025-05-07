use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let artifacts_dir = manifest_dir.join("flutter_artifacts");
    let include_dir = artifacts_dir.join("include");
    let header_windows = include_dir.join("flutter_windows.h");
    let header_embedder = include_dir.join("flutter_embedder.h");

    assert!(header_windows.is_file(), "flutter_windows.h not found");
    assert!(header_embedder.is_file(), "flutter_embedder.h not found");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Existing Windows bindings (DO NOT MODIFY ALLOWLIST)
    let bindings_windows = bindgen::Builder::default()
        .header(header_windows.to_str().unwrap())
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
        .expect("Unable to generate flutter_windows bindings");

    bindings_windows
        .write_to_file(out_dir.join("flutter_windows_bindings.rs"))
        .expect("Couldn't write flutter_windows bindings");

    // New embedder bindings (EXACT allowlist needed)
    let bindings_embedder = bindgen::Builder::default()
        .header(header_embedder.to_str().unwrap())
        .clang_arg(format!("-I{}", include_dir.display()))
        .allowlist_type("FlutterEngine.*")
        .allowlist_type("FlutterProjectArgs.*")
        .allowlist_type("FlutterSoftwareRendererConfig.*")
        .allowlist_type("FlutterSoftwareSurfacePresentCallback")
        .allowlist_type("FlutterRendererConfig.*")
        .allowlist_type("FlutterRendererType.*")
        .allowlist_type("FlutterEngineResult.*")
        .allowlist_type("FlutterWindowMetricsEvent*")
        .allowlist_function("FlutterEngine.*")
        .generate()
        .expect("Unable to generate flutter_embedder bindings");

    bindings_embedder
        .write_to_file(out_dir.join("flutter_embedder_bindings.rs"))
        .expect("Couldn't write flutter_embedder bindings");
}
