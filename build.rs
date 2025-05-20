use std::{env, path::PathBuf};

use keyboard_map::gen_keyboard_map::generate_keyboard_map;

mod keyboard_map;

fn main() {
    if let Err(e) = generate_keyboard_map("windows") {
        eprintln!("Error generating keyboard map: {}", e);
        std::process::exit(1);
    }

    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let keyboard_map_source_dir = manifest_dir.join("keyboard_map");
    println!(
        "cargo:rerun-if-changed={}",
        keyboard_map_source_dir
            .join("physical_key_data.g.json")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        keyboard_map_source_dir
            .join("logical_key_data.g.json")
            .display()
    );
    let gen_script_path = manifest_dir.join("build_utils").join("gen_keyboard_map.rs");
    println!("cargo:rerun-if-changed={}", gen_script_path.display());
    let artifacts_dir = manifest_dir.join("flutter_artifacts");
    let include_dir = artifacts_dir.join("include");
    let header_windows = include_dir.join("flutter_windows.h");
    let header_embedder = include_dir.join("flutter_embedder.h");

    generate_keyboard_map("windows").unwrap();

    assert!(header_windows.is_file(), "flutter_windows.h not found");
    assert!(header_embedder.is_file(), "flutter_embedder.h not found");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

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

    let bindings_embedder = bindgen::Builder::default()
        .header(header_embedder.to_str().unwrap())
        .clang_arg(format!("-I{}", include_dir.display()))
        .allowlist_type("FlutterEngine")
        .allowlist_type("FlutterProjectArgs")
        .allowlist_type("FlutterRendererConfig")
        .allowlist_type("FlutterWindowMetricsEvent")
        .allowlist_type("FlutterCustomTaskRunners")
        .allowlist_type("FlutterPointerEvent")
        .allowlist_type("FlutterPointerPhase")
        .allowlist_type("FlutterPointerDeviceKind")
        .allowlist_type("FlutterPointerSignalKind")
        .allowlist_type("FlutterKeyEvent")
        .allowlist_type("FlutterKeyEventDeviceType")
        .allowlist_type("FlutterKeyEventType")
        .allowlist_type("FlutterPlatformMessage")
        .allowlist_type("FlutterTaskRunnerDescription")
        .allowlist_type("FlutterEngineAOTDataSource")
        .allowlist_type("FlutterEngineAOTDataSourceType")
        .allowlist_type("FlutterEngineAOTDataSource__bindgen_ty_1")
        .allowlist_type("FlutterSoftwareRendererConfig")
        .allowlist_type("FlutterRendererType")
        .allowlist_type("FlutterTask")
        .allowlist_type("FlutterEngineResult")
        .allowlist_type("FlutterPlatformMessageResponseHandle")
        .allowlist_type("FlutterDesktopBinaryReply")
        .allowlist_type("FlutterSoftwareSurfacePresentCallback")
        .allowlist_function("FlutterEngineRun")
        .allowlist_function("FlutterEngineShutdown")
        .allowlist_function("FlutterEngineInitialize")
        .allowlist_function("FlutterEngineRunInitialized")
        .allowlist_function("FlutterEngineDeinitialize")
        .allowlist_function("FlutterEngineUpdateSemanticsEnabled")
        .allowlist_function("FlutterEngineSendWindowMetricsEvent")
        .allowlist_function("FlutterEngineGetCurrentTime")
        .allowlist_function("FlutterEngineSendPointerEvent")
        .allowlist_function("FlutterEngineSendKeyEvent")
        .allowlist_function("FlutterEngineSendPlatformMessage")
        .allowlist_function("FlutterEngineSendPlatformMessageResponse")
        .allowlist_function("FlutterPlatformMessageCreateResponseHandle")
        .allowlist_function("FlutterEngineCreateAOTData")
        .allowlist_function("FlutterEngineRunTask")
        .allowlist_function("FlutterEngineScheduleFrame")
        .allowlist_function("FlutterEngineOnVsync")
        .allowlist_function("FlutterEngineScheduleFrame")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate flutter_embedder bindings");

    bindings_embedder
        .write_to_file(out_dir.join("flutter_embedder_bindings.rs"))
        .expect("Couldn't write flutter_embedder bindings");
}
