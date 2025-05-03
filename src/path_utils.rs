//! Path utilities for Flutter desktop on Windows.
//!
//! Provides:
//! 1. Resolving default Flutter asset, ICU data and AOT library paths based on
//!    the directory where this DLL resides.
//! 2. Resolving those same paths from an arbitrary root directory.
//! 3. Displaying a native folder-picker dialog so the user can select a data directory.
//!
//! All returned paths are UTF-16, null-terminated vectors suitable for Win32 APIs.

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

/// Returns `(assets_path, icu_data_path, aot_library_path)` based on the DLL’s directory.
/// Panics if any of `data/{flutter_assets, icudtl.dat, app.so}` is missing.
pub fn get_flutter_paths() -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let dll_dir = dll_directory();
    get_flutter_paths_from(&dll_dir)
}

/// Like `get_flutter_paths()`, but uses `root_dir` instead of the DLL’s location.
/// Panics if `root_dir/data/{flutter_assets, icudtl.dat, app.so}` is missing.
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

/// Pops up the standard Windows “select folder” dialog and returns the chosen path,
/// or `None` if the user cancels.
pub fn select_data_directory() -> Option<PathBuf> {
    unsafe {
        // Initialize COM (no-op if already initialized).
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Create the folder-picker.
        let dialog: IFileOpenDialog = CoCreateInstance(
            &FileOpenDialog,
            None,
            CLSCTX_INPROC_SERVER,
        ).ok()?;

        // Only allow folder selection.
        dialog.SetOptions(FOS_PICKFOLDERS).ok()?;

        // Show the dialog (no owner window).
        dialog.Show(HWND(0)).ok()?;

        // Retrieve the selected item.
        let item = dialog.GetResult().ok()?;

        // Get its filesystem path.
        let buf = [0u16; MAX_PATH as usize];
        item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let os = OsString::from_wide(&buf[..len]);
        Some(PathBuf::from(os))
    }
}

/// Returns the directory containing this DLL, which serves as our default root.
pub fn dll_directory() -> PathBuf {
    unsafe {
        // Grab module handle by address of this function.
        let mut hmod = windows::Win32::Foundation::HMODULE(0);
        let addr = PCWSTR(dll_directory as *const () as _);
        let _ = GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            addr,
            &mut hmod,
        );

        // Query its file name.
        let mut buf = [0u16; MAX_PATH as usize];
        let len = GetModuleFileNameW(hmod, &mut buf) as usize;
        let os = OsString::from_wide(&buf[..len]);
        PathBuf::from(os).parent().unwrap().to_path_buf()
    }
}
