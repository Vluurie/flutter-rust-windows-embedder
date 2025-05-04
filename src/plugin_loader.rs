//! Dynamic plugin loader for Flutter desktop on Windows.
//!
//! This module discovers all plugin DLLs in a given directory by
//! scanning their export tables for symbols ending with
//! `RegisterWithRegistrar`, loads each DLL, and invokes those
//! registration functions so that each plugin is registered with
//! the Flutter engine at runtime.
//!
//! # Workflow
//!
//! 1. **discover_plugins**  
//!    Scan the release directory for `.dll` files, parse each as a PE
//!    image, and collect any exported symbols whose names end in
//!    `RegisterWithRegistrar`.
//!
//! 2. **load_and_register**  
//!    Load a single DLL, look up each `xxxRegisterWithRegistrar` symbol,
//!    and call it with the provided `FlutterDesktopPluginRegistrarRef`.
//!    The library handle is intentionally leaked to keep the DLL loaded
//!    for the lifetime of the process.
//!
//! 3. **load_and_register_plugins**  
//!    Tie it all together: discover plugins, retrieve the engine’s
//!    registrar, and register every discovered plugin DLL.
//!

use anyhow::{Context, Result};
use goblin::Object;
use libloading::{Library, Symbol};
use std::{
    ffi::CString,
    fs,
    path::{Path, PathBuf}, sync::Arc,
};

use crate::{dynamic_flutter_windows_dll_loader::FlutterDll, flutter_bindings::{
        FlutterDesktopEngineRef,
        FlutterDesktopPluginRegistrarRef,
    }};

const REG_SUFFIX: &str = "RegisterWithRegistrar";

/// Scan the given directory for all DLLs exporting any symbol ending
/// in `RegisterWithRegistrar`.
///
/// Returns a vector of `(dll_path, symbol_list)` for each DLL that
/// exports one or more matching symbols.
fn discover_plugins(release_dir: &Path) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(release_dir)
        .with_context(|| format!("reading directory {}", release_dir.display()))?
    {
        let dll = entry?.path();
        if dll.extension()
            .and_then(|e| e.to_str())
            .map_or(false, |e| e.eq_ignore_ascii_case("dll"))
        {
            let data = fs::read(&dll)
                .with_context(|| format!("reading {}", dll.display()))?;
            if let Object::PE(pe) = Object::parse(&data)? {
                let syms: Vec<String> = pe
                    .exports
                    .iter()
                    .filter_map(|e| e.name)
                    .filter(|n| n.ends_with(REG_SUFFIX))
                    .map(|s| s.to_string())
                    .collect();
                if !syms.is_empty() {
                    out.push((dll.clone(), syms));
                }
            }
        }
    }
    Ok(out)
}

/// Load the specified DLL and invoke each `xxxRegisterWithRegistrar`
/// symbol, passing in the given `registrar`.
///
/// The `Library` is leaked so the DLL remains loaded for the lifetime
/// of the process.
fn load_and_register(
    dll: &Path,
    symbols: &[String],
    registrar: FlutterDesktopPluginRegistrarRef,
) -> Result<()> {
    let lib = unsafe {
        Library::new(dll)
            .with_context(|| format!("loading {}", dll.display()))?
    };
    for sym in symbols {
        let cname = CString::new(sym.as_str()).unwrap();
        let func: Symbol<unsafe extern "C" fn(FlutterDesktopPluginRegistrarRef)> =
            unsafe {
                lib.get(cname.as_bytes_with_nul())
                    .with_context(|| format!("symbol {}", sym))?
            };
        unsafe { func(registrar) };
    }
    std::mem::forget(lib);
    Ok(())
}

/// Discover every plugin DLL in `release_dir` and register it with
/// the Flutter engine.
///
/// For each discovered DLL:
/// 1. Derive a plugin name from the DLL’s file stem (unused by the
///    current API but kept for future compatibility).
/// 2. Retrieve the engine’s plugin registrar via the dynamically
///    loaded `FlutterDesktopEngineGetPluginRegistrar` symbol.
/// 3. Load the DLL and invoke all its `RegisterWithRegistrar` symbols.
pub fn load_and_register_plugins(
    release_dir: &Path,
    engine: FlutterDesktopEngineRef,
    dll: &Arc<FlutterDll>,
) -> Result<()> {
    // Discover DLLs + matching symbols
    let plugins = discover_plugins(release_dir)
        .with_context(|| format!("discovering plugins in {}", release_dir.display()))?;


    for (dll_path, symbols) in plugins {
        // Derive plugin name from file stem (not currently used).
        let plugin_name = dll_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let _c_name = CString::new(plugin_name)?;

        // Call into the dynamic symbol instead of static extern
        let registrar: FlutterDesktopPluginRegistrarRef = unsafe {
            (dll.FlutterDesktopEngineGetPluginRegistrar)(engine, std::ptr::null())
        };

        load_and_register(&dll_path, &symbols, registrar)?;
    }
    Ok(())
}
