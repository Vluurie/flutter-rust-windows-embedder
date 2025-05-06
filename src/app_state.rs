//! Application state container for the embedded Flutter UI.

use std::sync::Arc;

use crate::{dynamic_flutter_windows_dll_loader::FlutterDll, flutter_bindings::FlutterDesktopViewControllerRef};
use windows::Win32::Foundation::HWND;

/// Holds the long‐lived handles needed to manage the Flutter view.
#[derive(Debug)]
pub struct AppState {
    /// The Flutter view controller managing the Flutter UI lifecycle.
    pub controller: FlutterDesktopViewControllerRef,
    /// The HWND of the child window where Flutter renders its content.
    pub child_hwnd: HWND,
    pub dll: Arc<FlutterDll>,
}
