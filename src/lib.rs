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
fn init_flutter_window() {

      Builder::from_env(Env::default().default_filter_or("debug"))
        .filter(None, LevelFilter::Debug)
        // turn on filter if you want to see goblin logs (.dll discovery imports etc..)
        .filter_module("goblin", LevelFilter::Off)
        .init();

    // --- COM Initialization (STA) ---
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED) {
            error!("COM initialization failed (STA): {:?}", e);
            std::process::exit(1);
        }
    }
    info!("COM initialized (STA)");

    // --- Flutter Engine Setup ---
    let engine = flutter_utils::create_flutter_engine();
    info!("Flutter engine created");

    // --- Flutter View Controller Setup ---
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

    // Get dll dir
    let dll_dir = flutter_utils::dll_directory();

    // Get the plugin registrar
    let registrar = unsafe {
        flutter_bindings::FlutterDesktopEngineGetPluginRegistrar(engine, std::ptr::null())
    };
    // Load and register plugins DYNAMICALLY in runtime !!!!!!
    if let Err(e) = plugin_loader::load_and_register_plugins(&dll_dir, registrar) {
        error!("Plugin loading failed: {:?}", e);
        std::process::exit(1); // rip
    }
    info!("Plugins loaded from {:?}", dll_dir);

    // --- Flutter View Embedding ---
    let (_view, flutter_child_hwnd) = flutter_utils::get_flutter_view_and_hwnd(controller);
    info!("Obtained Flutter child HWND: {:?}", flutter_child_hwnd);

    // --- Application State Setup ---
    let boxed_state = Box::new(AppState { controller, child_hwnd: flutter_child_hwnd });
    let app_state_ptr: *mut AppState = Box::into_raw(boxed_state);

    // --- Win32 Window Setup ---
    win32_utils::register_window_class();
    info!("Window class registered");

    let parent_hwnd = win32_utils::create_main_window(app_state_ptr);
    if parent_hwnd.0 == 0 {
        error!("Failed to create main window");
        // just do not leak :)
        unsafe { drop(Box::from_raw(app_state_ptr)) };
        std::process::exit(1);
    }
    info!("Main window created: {:?}", parent_hwnd);

    win32_utils::set_flutter_window_as_child(parent_hwnd, flutter_child_hwnd);
    info!(
        "Embedded Flutter HWND {:?} into parent {:?}",
        flutter_child_hwnd, parent_hwnd
    );

    // Show and focus the window
    unsafe {
        ShowWindow(parent_hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(parent_hwnd);
    }
    info!("Main window shown");

    // --- Message Loop ---
    win32_utils::run_message_loop(parent_hwnd, app_state_ptr);
    info!("Message loop exited");

    info!("Uninitializing COM");
    unsafe {
        CoUninitialize();
    }
    info!("Application exiting");
}
