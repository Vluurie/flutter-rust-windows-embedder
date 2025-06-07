#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use ::windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use ::windows::Win32::UI::WindowsAndMessaging::{SW_SHOWNORMAL, SetForegroundWindow, ShowWindow};
use env_logger::{Builder, Env};
use log::{LevelFilter, error, info};
use std::path::PathBuf;
use std::sync::Once;

mod app_state;
mod constants;
mod dynamic_flutter_windows_dll_loader;
mod flutter_utils;
pub mod path_utils;
mod plugin_loader;
pub mod software_renderer;
mod win32_utils;

pub mod windows {
    include!(concat!(env!("OUT_DIR"), "/flutter_windows_bindings.rs"));
}

pub mod embedder {
    include!(concat!(env!("OUT_DIR"), "/flutter_embedder_bindings.rs"));
}

static LOGGER_INIT: Once = Once::new();

/// Init logging for the enviroument debug filtered.
/// Calling this more than once is fine as it is already handled to be a Once call.
pub fn init_logging() {
    LOGGER_INIT.call_once(|| {
        Builder::from_env(Env::default().default_filter_or("debug"))
            .filter(None, LevelFilter::Debug)
            .filter_module("goblin", LevelFilter::Off)
            .init();
    });
}

/// Bootstraps a Flutter-powered window from the *default* DLL directory.
///
/// 1. Initializes COM (STA).  
/// 2. Probes `<dll_folder>/data/{flutter_assets,icudtl.dat,app.so}`.  
/// 3. Creates and configures the Flutter engine & view.  
/// 4. Scans **that same** DLL folder for plugin DLLs and registers them.  
/// 5. Embeds Flutter’s HWND, shows the window, and runs the message loop.
pub fn init_flutter_window() {
    init_flutter_window_from_dir(None)
}

/// Bootstraps a Flutter-powered window from a *custom* release directory.
///
/// - If `data_dir` is `Some(dir)`, then:
///   1. Reads `dir/data/flutter_assets`, `dir/data/icudtl.dat`, `dir/data/app.so`.  
///   2. Scans **that** `dir` for `*.dll` plugins.  
/// - If `data_dir` is `None`, falls back to the DLL’s own folder for both assets and plugins.
///
/// # Parameters
/// - `data_dir`: optional root path of your release bundle.
///
/// # Panics
/// Panics if any required asset is missing or engine/view creation fails.
pub fn init_flutter_window_from_dir(data_dir: Option<PathBuf>) {
    init_logging();

    // --- COM init (STA) ---
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).unwrap_or_else(|e| {
            error!("COM initialization failed: {:?}", e);
            std::process::exit(1);
        });
    }
    info!("COM initialized (STA)");

    // 1) Resolve Flutter asset paths
    let (assets, icu, aot) = match data_dir.as_ref() {
        Some(dir) => path_utils::get_flutter_paths_from(dir),
        None => path_utils::get_flutter_paths(),
    };

    let dir_ref = data_dir.as_deref();
    let dll =
        dynamic_flutter_windows_dll_loader::FlutterDll::get_for(dir_ref).unwrap_or_else(|e| {
            error!(
                "Failed to load flutter_windows.dll from `{:?}`: {:?}",
                dir_ref, e
            );
            std::process::exit(1);
        });

    info!(
        "Loaded flutter_windows.dll from `{}`",
        dir_ref
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "EXE folder".into())
    );

    // 2) Create the engine with explicit paths
    let engine = flutter_utils::create_flutter_engine_with_paths(assets, icu, aot, &dll);
    info!("Flutter engine created");

    // 3) Create the view controller
    let controller = flutter_utils::create_flutter_view_controller(
        engine,
        constants::DEFAULT_WINDOW_WIDTH,
        constants::DEFAULT_WINDOW_HEIGHT,
        &dll,
    );
    info!(
        "Flutter view controller created ({}×{})",
        constants::DEFAULT_WINDOW_WIDTH,
        constants::DEFAULT_WINDOW_HEIGHT
    );

    // 4) Register plugins from the same directory
    let binding = path_utils::dll_directory();
    let plugin_dir = data_dir.as_ref().unwrap_or(&binding);
    plugin_loader::load_and_register_plugins(plugin_dir, engine, Some(&dll)).unwrap_or_else(|e| {
        error!(
            "Plugin load failed from `{}`: {:?}",
            plugin_dir.display(),
            e
        );
        std::process::exit(1);
    });
    info!("All plugins registered from `{}`", plugin_dir.display());

    // 5) Embed Flutter’s HWND in a Win32 window
    let (_view, flutter_child_hwnd) = flutter_utils::get_flutter_view_and_hwnd(controller, &dll);
    let state = Box::new(app_state::AppState {
        controller,
        child_hwnd: flutter_child_hwnd,
        dll: dll.clone(),
    });
    let state_ptr = Box::into_raw(state);

    win32_utils::register_window_class();
    let parent_hwnd = win32_utils::create_main_window(state_ptr);
    win32_utils::set_flutter_window_as_child(parent_hwnd, flutter_child_hwnd);

    // 6) Show and enter the message loop
    unsafe {
        ShowWindow(parent_hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(parent_hwnd);
    }
    info!("Main window shown");

    win32_utils::run_message_loop(parent_hwnd, state_ptr);
    info!("Message loop exited");

    unsafe {
        CoUninitialize();
    }
    info!("Application exiting");
}
