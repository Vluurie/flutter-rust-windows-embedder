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
    // --- 0) One‑time logging + COM (STA) init ---
    init_logging();
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED) {
            error!("COM init failed (STA): {:?}", e);
            std::process::exit(1);
        }
    }
    info!("COM initialized (STA)");

    // --- 1) Create the Flutter engine (no Dart VM yet) ---
    let engine = flutter_utils::create_flutter_engine();
    info!("Flutter engine created");

    // --- 2) Grab the *one* PluginRegistrar from the engine ---
    //     This pointer will later hold both method‑channel and texture registrars.
    let registrar = unsafe {
        flutter_bindings::FlutterDesktopEngineGetPluginRegistrar(engine, std::ptr::null())
    };

    // --- Phase 1: Register engine‑only plugins ---
    //   (pure MethodChannel plugins; must run before Dart boots)
    plugin_loader::load_and_register_selected(
        &flutter_utils::dll_directory(),
        registrar,
        |syms| !syms.iter().any(|s| s.contains("TextureRegistrar")),
    )
    .unwrap_or_else(|e| {
        error!("Engine‑only plugin registration failed: {:?}", e);
        std::process::exit(1);
    });
    info!("Engine‑only plugins registered");

    // --- Phase 2: Run the engine + create the view controller ---
    //   Under the hood, this calls FlutterDesktopRunEngine,
    //   which attaches the texture registrar into `registrar`.
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

    // --- Phase 3: Register texture/view‑dependent plugins ---
    //   Now that `registrar` has a valid texture registrar,
    //   we can safely register plugins like window_manager.
    plugin_loader::load_and_register_selected(
        &flutter_utils::dll_directory(),
        registrar,
        |syms| syms.iter().any(|s| s.contains("TextureRegistrar")),
    )
    .unwrap_or_else(|e| {
        error!("Texture plugin registration failed: {:?}", e);
        std::process::exit(1);
    });
    info!("Texture/view‑dependent plugins registered");

    // --- 4) Embed the Flutter child HWND into our Win32 window ---
    let (_view, flutter_child_hwnd) = flutter_utils::get_flutter_view_and_hwnd(controller);
    let boxed = Box::new(AppState { controller, child_hwnd: flutter_child_hwnd });
    let app_state_ptr = Box::into_raw(boxed);

    win32_utils::register_window_class();
    let parent = win32_utils::create_main_window(app_state_ptr);
    if parent.0 == 0 {
        error!("Failed to create main window");
        unsafe { drop(Box::from_raw(app_state_ptr)) };
        std::process::exit(1);
    }
    win32_utils::set_flutter_window_as_child(parent, flutter_child_hwnd);

    // --- 5) Show & focus the window (now Flutter will paint) ---
    unsafe {
        ShowWindow(parent, SW_SHOWNORMAL);
        SetForegroundWindow(parent);
    }
    info!("Main window shown");

    // --- 6) Message loop + cleanup ---
    win32_utils::run_message_loop(parent, app_state_ptr);
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
