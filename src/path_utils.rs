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
//!   `data/flutter_assets`, `data/icudtl.dat` and `data/app.so`, and
//!   return their paths. Panics if any of these are missing.
//!
//! - **`get_flutter_paths_from(root_dir: &Path)`**  
//!   Same as `get_flutter_paths()`, but rooted at an arbitrary
//!   `root_dir` instead of the DLL’s location.
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
//! if the expected directory structure or files are not present:
//! ```text
//! <root_dir>/data/flutter_assets/
//! <root_dir>/data/icudtl.dat
//! <root_dir>/data/app.so
//! ```
//!
//! If you need more control (for example, falling back when a directory
//! is missing), call `select_data_directory()` first and then route its
//! result into `get_flutter_paths_from(...)`.

use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HWND, MAX_PATH},
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
            LibraryLoader::{
                GetModuleFileNameW, GetModuleHandleExW,
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            },
        },
        UI::Shell::{FileOpenDialog, FOS_PICKFOLDERS, IFileOpenDialog, SIGDN_FILESYSPATH},
    },
};

/// Returns `(assets_path, icu_data_path, aot_library_path)` by inspecting
/// the folder where this DLL (or EXE) is located.  
/// Panics if any of the following are missing:
///
/// - `data/flutter_assets/`  
/// - `data/icudtl.dat`  
/// - `data/app.so`
pub fn get_flutter_paths() -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let dll_dir = dll_directory();
    get_flutter_paths_from(&dll_dir)
}

/// Like `get_flutter_paths()`, but rooted at the provided `root_dir`.
/// Use this when you want to supply your own data folder (for example,
/// after the user selects it via `select_data_directory()`).  
/// Panics if the same files/directories above are not found under
/// `root_dir/data/`.
pub fn get_flutter_paths_from(root_dir: &Path) -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let data_dir   = root_dir.join("data");
    let assets_dir = data_dir.join("flutter_assets");
    let icu_file   = data_dir.join("icudtl.dat");
    let aot_lib    = data_dir.join("app.so");

    if !assets_dir.is_dir() {
        panic!(
            "[Path Utils] Missing `flutter_assets` at `{}`",
            assets_dir.display()
        );
    }
    if !icu_file.is_file() {
        panic!(
            "[Path Utils] Missing `icudtl.dat` at `{}`",
            icu_file.display()
        );
    }
    if !aot_lib.is_file() {
        panic!(
            "[Path Utils] Missing AOT library `app.so` at `{}`",
            aot_lib.display()
        );
    }

    fn to_wide(p: &Path) -> Vec<u16> {
        OsString::from(p)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    (to_wide(&assets_dir), to_wide(&icu_file), to_wide(&aot_lib))
}

/// Displays the standard Windows “Select Folder” dialog.  
/// Returns `Some(PathBuf)` if the user picks a folder, or `None` if
/// they cancel.
pub fn select_data_directory() -> Option<PathBuf> {
    unsafe {
        // Make sure COM is initialized (STA).
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Create and configure the folder-picker.
        let dialog: IFileOpenDialog = CoCreateInstance(
            &FileOpenDialog,
            None,
            CLSCTX_INPROC_SERVER,
        ).ok()?;
        dialog.SetOptions(FOS_PICKFOLDERS).ok()?;

        // Show it (no owner window).
        dialog.Show(HWND(0)).ok()?;

        // Get the selected item and its filesystem path.
        let item = dialog.GetResult().ok()?;
        let buf = [0u16; MAX_PATH as usize];
        item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(PathBuf::from(OsString::from_wide(&buf[..len])))
    }
}

/// Returns the directory containing this DLL (or executable).  
/// Internally uses `GetModuleHandleExW` and `GetModuleFileNameW` to
/// locate the module by the address of this function.
pub fn dll_directory() -> PathBuf {
    unsafe {
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
        PathBuf::from(OsString::from_wide(&buf[..len])).parent().unwrap().to_path_buf()
    }
}
