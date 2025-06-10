
#[allow(unused_imports)]
use std::{env, path::PathBuf};

mod keyboard_map;

#[cfg(feature = "regenerate-bindings")]
mod regenerate_assets {
    use super::keyboard_map;
    use keyboard_map::gen_keyboard_map::generate_keyboard_map;
    use std::{env, path::PathBuf};

    pub fn run() {
        println!("cargo:warning=Feature 'regenerate-bindings' is active. Regenerating all assets...");

        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let bindings_out_path = manifest_dir.join("src").join("bindings");
        if !bindings_out_path.exists() {
            std::fs::create_dir_all(&bindings_out_path).expect("Could not create bindings directory");
        }

        let keyboard_map_out_path = bindings_out_path.join("keyboard_layout.rs");
        if let Err(e) = generate_keyboard_map("windows", &keyboard_map_out_path) {
            eprintln!("Error generating keyboard map: {}", e);
            std::process::exit(1);
        }
        println!("cargo:warning=Keyboard map has been regenerated into {}.", keyboard_map_out_path.display());

        let artifacts_dir = manifest_dir.join("flutter_artifacts");
        let include_dir = artifacts_dir.join("include");
        let header_windows = include_dir.join("flutter_windows.h");
        let header_embedder = include_dir.join("flutter_embedder.h");

        assert!(header_windows.is_file(), "flutter_windows.h not found");
        assert!(header_embedder.is_file(), "flutter_embedder.h not found");
        
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
            .write_to_file(bindings_out_path.join("flutter_windows_bindings.rs"))
            .expect("Couldn't write flutter_windows bindings");

        let bindings_embedder = bindgen::Builder::default()
            .header(header_embedder.to_str().unwrap())
            .clang_arg(format!("-I{}", include_dir.display()))
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
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
            .allowlist_type("FlutterEngineDartObject")
            .allowlist_function("FlutterEnginePostDartObject")
            .allowlist_type("FlutterEngineDartPort")
            .allowlist_type("FlutterEngineDartBuffer")
            .allowlist_type("FlutterEngineDartObjectType")
            .generate()
            .expect("Unable to generate flutter_embedder bindings");

        bindings_embedder
            .write_to_file(bindings_out_path.join("flutter_embedder_bindings.rs"))
            .expect("Couldn't write flutter_embedder bindings");
        
        println!("cargo:warning=Bindings have been regenerated in src/bindings/.");

        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=keyboard_map/mod.rs");
        println!("cargo:rerun-if-changed=keyboard_map/gen_keyboard_map.rs");
    }
}

fn main() {
    #[cfg(feature = "regenerate-bindings")]
    regenerate_assets::run();
}