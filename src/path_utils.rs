//! Path-related utilities for Flutter desktop on Windows.
//!
//! This module helps you locate Flutter’s runtime assets on disk and,
//! if needed, prompt the user to pick a custom data directory. All
//! returned paths are UTF-16, null-terminated `Vec<u16>` values suitable
//! for passing directly into Win32 APIs.
//!
//! # Available functions
//!
//! - **`get_flutter_paths()`**  
//!   Inspect the folder where your DLL (or EXE) lives, look for
//!   `data/flutter_assets` and `data/icudtl.dat`, and return their paths.
//!   If *both* these are present, it then looks for `data/app.so`
//!   - Panics if *either* `flutter_assets` or `icudtl.dat` is missing.  
//!   - If the AOT library is not found, logs an informational message and
//!     returns an empty `Vec<u16>` for the AOT path, indicating the engine
//!     should fall back to JIT mode.
//!
//! - **`get_flutter_paths_from(root_dir: &Path)`**  
//!   Same as `get_flutter_paths()`, but rooted at an arbitrary
//!   `root_dir` instead of the DLL’s location.  
//!   - Panics if `data/flutter_assets` or `data/icudtl.dat` are missing
//!     under `root_dir`.  
//!   - Logs and returns an empty `Vec<u16>` for the AOT path if
//!     `data/app.so is not found, causing JIT fallback.
//!
//! - **`select_data_directory()`**  
//!   Opens the standard Windows “Select Folder” dialog and returns the
//!   chosen folder as a `PathBuf`, or `None` if the user cancels.
//!
//! - **`dll_directory()`**  
//!   Returns the filesystem path of the directory containing this DLL
//!   (or executable). You can use this as the default root for
//!   `get_flutter_paths_from`.
//!
//! # Panics
//!
//! Both `get_flutter_paths()` and `get_flutter_paths_from(...)` will panic
//! if the required **non‑AOT** assets are missing:
//! ```text
//! <root_dir>/data/flutter_assets/
//! <root_dir>/data/icudtl.dat
//! ```
//! If the AOT library file (`app.so`) is missing, they will
//! *not* panic but instead log an info message and return an empty
//! `Vec<u16>` for the AOT path (triggering JIT mode).

use log::{debug, error, info};
use std::{
    ffi::{c_void, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HWND, MAX_PATH},
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
            LibraryLoader::{
                GetModuleFileNameW, GetModuleHandleExW,
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            },
        },
        UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_PICKFOLDERS, SIGDN_FILESYSPATH},
    },
};

/// Returns `(assets_path, icu_data_path, aot_library_path)` by inspecting
/// the folder where this DLL (or EXE) is located.
/// Panics if `flutter_assets` or `icudtl.dat` are missing.  
/// If the AOT library isn’t found, logs an info message and returns
/// an empty `Vec<u16>` for the AOT path (→ JIT).
pub fn get_flutter_paths() -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let dll_dir = dll_directory();
    info!("[Path Utils] Using DLL directory as root: {}", dll_dir.display());
    get_flutter_paths_from(&dll_dir)
}

/// Like `get_flutter_paths()`, but rooted at the provided `root_dir`.
/// Panics if `flutter_assets` or `icudtl.dat` are missing under `root_dir`.
/// Logs and returns an empty `Vec<u16>` for the AOT path if `app.so`
/// is not found, causing JIT fallback.
pub fn get_flutter_paths_from(root_dir: &Path) -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    info!("[Path Utils] Resolving Flutter paths under `{}`", root_dir.display());

    let data_dir   = root_dir.join("data");
    let assets_dir = data_dir.join("flutter_assets");
    let icu_file   = data_dir.join("icudtl.dat");
    let aot_lib    = data_dir.join("app.so"); // or `.dll` on Windows

    debug!("[Path Utils] Checking for `{}`", assets_dir.display());
    if !assets_dir.is_dir() {
        error!("[Path Utils] Missing directory `{}`", assets_dir.display());
        panic!("[Path Utils] Missing `flutter_assets` at `{}`", assets_dir.display());
    }

    debug!("[Path Utils] Checking for `{}`", icu_file.display());
    if !icu_file.is_file() {
        error!("[Path Utils] Missing file `{}`", icu_file.display());
        panic!("[Path Utils] Missing `icudtl.dat` at `{}`", icu_file.display());
    }

    debug!("[Path Utils] Checking for `{}`", aot_lib.display());
    let aot_w = if aot_lib.is_file() {
        to_wide(&aot_lib)
    } else {
        info!(
            "[Path Utils] AOT library not found at `{}`, falling back to JIT mode",
            aot_lib.display()
        );
        Vec::new()
    };

    let assets = to_wide(&assets_dir);
    let icu    = to_wide(&icu_file);

    info!("[Path Utils] Resolved Flutter asset paths successfully");
    (assets, icu, aot_w)
}

/// Helper: turn a Path → Vec<u16> with trailing NUL
fn to_wide(p: &Path) -> Vec<u16> {
    OsString::from(p)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Displays the standard Windows “Select Folder” dialog.
/// Returns `Some(PathBuf)` if the user picks a folder, or `None` if
/// they cancel.
pub fn select_data_directory() -> Option<PathBuf> {
    unsafe {
        info!("[Path Utils] Showing folder-picker dialog");
        // Make sure COM is initialized (STA).
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Create and configure the folder-picker.
        let dialog: IFileOpenDialog = CoCreateInstance(
            &FileOpenDialog,
            None,
            CLSCTX_INPROC_SERVER,
        ).ok()?;
        dialog.SetOptions(FOS_PICKFOLDERS).ok()?;
        debug!("[Path Utils] Folder-picker configured for folders only");

        // Show it (no owner window).
        dialog.Show(HWND(0)).ok()?;
        debug!("[Path Utils] Folder-picker accepted");

        // Get the selected item.
        let item = dialog.GetResult().ok()?;

        // Ask for its file-system path.
        let pwstr = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| {
                error!("[Path Utils] GetDisplayName failed: {:?}", e);
                e
            })
            .ok()?;

        // Convert the PWSTR (null-terminated) into a Rust OsString / PathBuf.
        let mut len = 0;
        while *pwstr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(pwstr.0, len);
        let os = OsString::from_wide(slice);
        debug!("[Path Utils] User selected directory: {}", os.to_string_lossy());

        // Free the string that COM allocated.
        CoTaskMemFree(Some(pwstr.0.cast::<c_void>() as *const _));

        Some(PathBuf::from(os))
    }
}

/// Returns the directory containing this DLL (or executable).
/// Internally uses Win32 APIs to locate the module by the address of this function.
pub fn dll_directory() -> PathBuf {
    unsafe {
        debug!("[Path Utils] Locating module filename via Win32 APIs");
        let mut hmod = windows::Win32::Foundation::HMODULE(0);
        let addr = PCWSTR(dll_directory as *const () as _);
        let _ = GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            addr,
            &mut hmod,
        );

        let mut buf = [0u16; MAX_PATH as usize];
        let len = GetModuleFileNameW(hmod, &mut buf) as usize;
        let os: OsString = OsString::from_wide(&buf[..len]);
        let path = PathBuf::from(os);
        let dir = path.parent().unwrap().to_path_buf();
        info!("[Path Utils] Module is at `{}`", path.display());
        info!("[Path Utils] DLL directory = `{}`", dir.display());
        dir
    }
}
