#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)] // ignores bindings code style
#![allow(dead_code)] // ignores unused Flutter bindings dead code
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")] // Remove this for Console in Debug - Keep in Release

/// A Windows host application that embeds a Flutter view.
/// 
/// - Initializes COM (STA) for Flutter plugins and Win32 operations  
/// - Creates the Flutter engine and view controller  
/// - Loads and registers Flutter plugins found beside the executable  
/// - Hosts the Flutter child HWND in a native Win32 parent window  
/// - Runs the standard message loop and cleans up on exit

mod app_state;
mod constants;
mod flutter_utils;
mod win32_utils;
mod plugin_loader;

mod flutter_bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use std::sync::Once;

use log::{error, info, LevelFilter};
use env_logger::{self, Builder, Env};
use app_state::AppState;
use windows::Win32::{
    System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
    UI::WindowsAndMessaging::{ShowWindow, SetForegroundWindow, SW_SHOWNORMAL},
};

/// Program entry point.
/// 
/// 1. Initialize logging and COM.  
/// 2. Create Flutter engine and view controller.  
/// 3. Load and register plugins alongside the executable.  
/// 4. Embed the Flutter HWND in a Win32 window.  
/// 5. Show the window and run the message loop.  
/// 6. Uninitialize COM and exit.
pub fn init_flutter_window() {
    init_logging();

    // --- 1) COM Init ---
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED) {
            error!("COM init failed: {:?}", e);
            std::process::exit(1);
        }
    }
    info!("COM initialized (STA)");

    // --- 2) Create engine (no Dart yet) ---
    let engine = flutter_utils::create_flutter_engine();
    info!("Flutter engine created");

    // --- 3) Grab the one registrar pointer from the engine ---
    //     (texture registrar will attach to this later)
    let registrar = unsafe {
        flutter_bindings::FlutterDesktopEngineGetPluginRegistrar(engine, std::ptr::null())
    };

    // --- 4) Phase 1: Engine‑only plugins (method channels) ---
    //     Register _before_ we ever boot Dart.
    plugin_loader::load_and_register_selected(
        &flutter_utils::dll_directory(),
        registrar,
        |symbols| !symbols.iter().any(|s| s.contains("TextureRegistrar")),
    )
    .unwrap_or_else(|e| {
        error!("Engine‑only plugin registration failed: {:?}", e);
        std::process::exit(1);
    });
    info!("Engine‑only plugins registered");

    // --- 5) Create the view controller (this will under the hood call RunEngine) ---
    let controller = flutter_utils::create_flutter_view_controller(
        engine,
        constants::DEFAULT_WINDOW_WIDTH,
        constants::DEFAULT_WINDOW_HEIGHT,
    );
    info!(
        "Flutter view controller created ({}×{})",
        constants::DEFAULT_WINDOW_WIDTH,
        constants::DEFAULT_WINDOW_HEIGHT
    );

    // --- 6) Embed the Flutter HWND into our Win32 window but don’t show yet ---
    let (_view, flutter_child_hwnd) = flutter_utils::get_flutter_view_and_hwnd(controller);
    let boxed_state = Box::new(AppState { controller, child_hwnd: flutter_child_hwnd });
    let app_state_ptr: *mut AppState = Box::into_raw(boxed_state);

    win32_utils::register_window_class();
    let parent_hwnd = win32_utils::create_main_window(app_state_ptr);
    win32_utils::set_flutter_window_as_child(parent_hwnd, flutter_child_hwnd);

    // --- 7) Show the window — at this point Flutter has initialized textures. ---
    unsafe {
        ShowWindow(parent_hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(parent_hwnd);
    }
    info!("Main window shown");

    // --- 8) Phase 2: View‑dependent plugins (textures, platform views, etc.) ---
    //     Now that the view is shown, the texture registrar is alive on `registrar`.
    plugin_loader::load_and_register_selected(
        &flutter_utils::dll_directory(),
        registrar,
        |symbols| symbols.iter().any(|s| s.contains("TextureRegistrar")),
    )
    .unwrap_or_else(|e| {
        error!("View‑dependent plugin registration failed: {:?}", e);
        std::process::exit(1);
    });
    info!("View‑dependent plugins registered");

    // --- 9) Enter the message loop (Dart is running, plugins are hooked) ---
    win32_utils::run_message_loop(parent_hwnd, app_state_ptr);

    // --- Cleanup ---
    info!("Uninitializing COM");
    unsafe { CoUninitialize() };
    info!("Application exiting");
}


// when we init loggin on first flutter app start then close the app and reopen another one
// from the same rust process, we get init log error so we make it static and only init once
static LOGGER_INIT: Once = Once::new();

fn init_logging() {
    LOGGER_INIT.call_once(|| {
        Builder::from_env(Env::default().default_filter_or("debug"))
            .filter(None, LevelFilter::Debug)
            .filter_module("goblin", LevelFilter::Off)
            .init();
    });
}
