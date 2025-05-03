#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]
#![allow(dead_code)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

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
use env_logger::{Builder, Env};
use app_state::AppState;
use windows::Win32::{
    System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
    UI::WindowsAndMessaging::{ShowWindow, SetForegroundWindow, SW_SHOWNORMAL},
};

pub fn init_flutter_window() {
    init_logging();

    // --- COM init ---
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .unwrap_or_else(|e| {
                error!("COM initialization failed: {:?}", e);
                std::process::exit(1);
            });
    }
    info!("COM initialized (STA)");

    // 1) Create the engine
    let engine = flutter_utils::create_flutter_engine();
    info!("Flutter engine created");

    // 2) Register engine‐only plugins
    let dll_dir = flutter_utils::dll_directory();
    plugin_loader::load_engine_plugins(&dll_dir, engine)
        .unwrap_or_else(|e| {
            error!("Engine plugin load failed: {:?}", e);
            std::process::exit(1);
        });
    info!("Engine plugins registered");

    // 3) Create the view controller
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

    // 4) Register view‐level plugins using the same engine registrar
    plugin_loader::load_view_plugins(&dll_dir, engine)
        .unwrap_or_else(|e| {
            error!("View plugin load failed: {:?}", e);
            std::process::exit(1);
        });
    info!("View plugins registered");

    // 5) Obtain child HWND, embed, show, message loop
    let (_view, flutter_child_hwnd) = flutter_utils::get_flutter_view_and_hwnd(controller);
    let boxed = Box::new(AppState { controller, child_hwnd: flutter_child_hwnd });
    let state_ptr: *mut AppState = Box::into_raw(boxed);

    win32_utils::register_window_class();
    let parent_hwnd = win32_utils::create_main_window(state_ptr);
    win32_utils::set_flutter_window_as_child(parent_hwnd, flutter_child_hwnd);

    unsafe {
        ShowWindow(parent_hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(parent_hwnd);
    }
    info!("Main window shown");

    win32_utils::run_message_loop(parent_hwnd, state_ptr);
    info!("Message loop exited");

    unsafe { CoUninitialize(); }
    info!("Application exiting");
}

static LOGGER_INIT: Once = Once::new();
fn init_logging() {
    LOGGER_INIT.call_once(|| {
        Builder::from_env(Env::default().default_filter_or("debug"))
            .filter(None, LevelFilter::Debug)
            .filter_module("goblin", LevelFilter::Off)
            .init();
    });
}
