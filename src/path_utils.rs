use log::{debug, error, info};
use std::{
    ffi::{OsString, c_void},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows::{
    Win32::{
        Foundation::{HWND, MAX_PATH},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree,
            },
            LibraryLoader::{
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
                GetModuleHandleExW,
            },
        },
        UI::Shell::{FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH},
    },
    core::PCWSTR,
};

/// Returns `(assets_path, icu_data_path, aot_library_path)` by inspecting
/// the folder where this DLL (or EXE) is located.
/// This assumes the flutter app was build with flutter build xxxx --debug or release
/// Panics if any of the following are missing:
///
/// - `data/flutter_assets/`
/// - `data/icudtl.dat`
/// - `data/app.so`
pub fn get_flutter_build_paths() -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let dll_dir = dll_directory();
    get_flutter_build_paths_from(&dll_dir)
}

/// Like `get_flutter_paths()`, but rooted at the provided `root_dir`.
/// Checks for both `assemble` and `build` layouts.
/// Panics if `flutter_assets` or `icudtl.dat` are not found under `root_dir/data/`;
/// but if `app.so` is missing, falls back to JIT mode (returns an empty AOT path).
pub fn get_flutter_build_paths_from(root_dir: &Path) -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let assets_dir: PathBuf;
    let icu_file: PathBuf;
    let aot_lib: PathBuf;

    let assemble_assets_dir = root_dir.join("flutter_assets");
    if assemble_assets_dir.is_dir() {
        info!("[Path Utils] Detected 'flutter assemble' asset layout.");
        assets_dir = assemble_assets_dir;
        icu_file = root_dir.join("icudtl.dat");
        aot_lib = root_dir.join("windows").join("app.so");
    } else {
        info!("[Path Utils] 'assemble' layout not found, falling back to 'flutter build' layout.");
        let data_dir = root_dir.join("data");
        assets_dir = data_dir.join("flutter_assets");
        icu_file = data_dir.join("icudtl.dat");
        aot_lib = data_dir.join("app.so");
    }

    debug!("[Path Utils] Using asset root: `{}`", assets_dir.display());

    // 1) flutter_assets must exist
    if !assets_dir.is_dir() {
        error!("[Path Utils] Missing directory `{}`", assets_dir.display());
        panic!(
            "[Path Utils] Missing `flutter_assets` at `{}`",
            assets_dir.display()
        );
    }

    // 2) icudtl.dat must exist
    if !icu_file.is_file() {
        error!("[Path Utils] Missing file `{}`", icu_file.display());
        panic!(
            "[Path Utils] Missing `icudtl.dat` at `{}`",
            icu_file.display()
        );
    }

    // 3) app.so (AOT lib) is optional — fall back to JIT if missing
    let aot_path_vec = if aot_lib.is_file() {
        info!("[Path Utils] Found AOT library at `{}`", aot_lib.display());
        to_wide(&aot_lib)
    } else {
        info!(
            "[Path Utils] AOT library not found at `{}`, falling back to JIT mode",
            aot_lib.display()
        );
        Vec::new()
    };

    fn to_wide(p: &Path) -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    info!("[Path Utils] Resolved Flutter asset paths successfully");
    (to_wide(&assets_dir), to_wide(&icu_file), aot_path_vec)
}

/// Displays the standard Windows “Select Folder” dialog.
/// Returns `Some(PathBuf)` if the user picks a folder, or `None` if
/// they cancel.
pub fn select_data_directory() -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.SetOptions(FOS_PICKFOLDERS).ok()?;
        dialog.Show(Some(HWND(null_mut()))).ok()?;
        let item = dialog.GetResult().ok()?;
        let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let len = (0..).take_while(|&i| *pwstr.0.add(i) != 0).count();
        let slice = std::slice::from_raw_parts(pwstr.0, len);
        let os = OsString::from_wide(slice);
        CoTaskMemFree(Some(pwstr.0 as *const c_void));
        Some(PathBuf::from(os))
    }
}

/// Returns the directory containing this DLL (or executable).
/// Internally uses `GetModuleHandleExW` and `GetModuleFileNameW` to
/// locate the module by the address of this function.
pub fn dll_directory() -> PathBuf {
    unsafe {
        let mut hmod = windows::Win32::Foundation::HMODULE(null_mut());
        let addr_of_this_func = dll_directory as *const ();
        let _ = GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(addr_of_this_func as *const u16),
            &mut hmod,
        );
        let mut buf = vec![0u16; MAX_PATH as usize];
        let len = GetModuleFileNameW(Some(hmod), &mut buf) as usize;
        let os: OsString = OsString::from_wide(&buf[..len]);
        let path = PathBuf::from(os);
        path.parent().unwrap().to_path_buf()
    }
}

/// Returns (assets, icu, aot) as Vec<u16> with trailing NULs stripped on conversion.
/// Panics if assets or icu missing; returns empty Vec for aot if missing.
pub fn load_flutter_build_paths(
    data_dir: Option<PathBuf>,
) -> (OsString, OsString, Option<OsString>) {
    let (assets_w, icu_w, aot_w) = match data_dir.as_ref() {
        Some(dir) => get_flutter_build_paths_from(dir),
        None => get_flutter_build_paths(),
    };
    let strip = |mut v: Vec<u16>| {
        if v.last() == Some(&0) {
            v.pop();
        }
        v
    };
    let os_from = |v: Vec<u16>| OsString::from_wide(&strip(v));
    let assets_os = os_from(assets_w);
    let icu_os = os_from(icu_w);
    let aot_vec = strip(aot_w);
    let aot_os = if aot_vec.is_empty() {
        None
    } else {
        Some(OsString::from_wide(&aot_vec))
    };
    (assets_os, icu_os, aot_os)
}
