use windows::core::{w, PCWSTR};

/// The Win32 window class name used to register and create the main window.
pub const WINDOW_CLASS_NAME: PCWSTR = w!("FLUTTER_RUST_EMBEDDER_WINDOW");

/// Default width (in pixels) for the main application window.
pub const DEFAULT_WINDOW_WIDTH: i32 = 1280;

/// Default height (in pixels) for the main application window.
pub const DEFAULT_WINDOW_HEIGHT: i32 = 720;

/// Title text for the main application window.
pub const WINDOW_TITLE: PCWSTR = w!("Flutter Rust App");

/// arguments passed to the Dart entrypoint.
/// Adjust or extend as needed for your application.
/// b"--verbose-logging\0", as example var
pub const DART_ENTRYPOINT_ARGS: &[&[u8]] = &[];
