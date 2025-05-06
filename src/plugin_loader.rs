//! Dynamic plugin loader for Flutter desktop on Windows.
//!
//! This module discovers all plugin DLLs in a given directory by
//! scanning their export tables for symbols ending with
//! `RegisterWithRegistrar`, loads each DLL, and invokes those
//! registration functions so that each plugin is registered exactly
//! once per Flutter engine at runtime.
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
//!    registrar, and register every discovered plugin DLL, but only
//!    once *per engine* (so you can create multiple engines without
//!    double‑registering the same plugin into one engine).
//!

use anyhow::{Context, Result};
use goblin::Object;
use libloading::{Library, Symbol};
use std::{
    collections::{HashMap, HashSet},
    ffi::CString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use crate::{
    dynamic_flutter_windows_dll_loader::FlutterDll,
    windows::{FlutterDesktopEngineRef, FlutterDesktopPluginRegistrarRef},
};

const REG_SUFFIX: &str = "RegisterWithRegistrar";

/// Global map: engine_ptr (usize) → set of plugin names already registered.
static REGISTERED_PLUGINS: OnceLock<Mutex<HashMap<usize, HashSet<String>>>> = OnceLock::new();

/// Initialize or fetch our global registry.
fn registered_map() -> &'static Mutex<HashMap<usize, HashSet<String>>> {
    REGISTERED_PLUGINS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Scan the given directory for all DLLs exporting any symbol ending
/// in `RegisterWithRegistrar`.
fn discover_plugins(release_dir: &Path) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(release_dir)
        .with_context(|| format!("reading directory {}", release_dir.display()))?
    {
        let dll = entry?.path();
        if dll
            .extension()
            .and_then(|e| e.to_str())
            .map_or(false, |e| e.eq_ignore_ascii_case("dll"))
        {
            let data = fs::read(&dll).with_context(|| format!("reading {}", dll.display()))?;
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
/// symbol, passing in the given `registrar`. Leaks the Library.
fn load_and_register(
    dll: &Path,
    symbols: &[String],
    registrar: FlutterDesktopPluginRegistrarRef,
) -> Result<()> {
    let lib = unsafe { Library::new(dll).with_context(|| format!("loading {}", dll.display()))? };
    for sym in symbols {
        let cname = CString::new(sym.as_str()).unwrap();
        let func: Symbol<unsafe extern "C" fn(FlutterDesktopPluginRegistrarRef)> = unsafe {
            lib.get(cname.as_bytes_with_nul())
                .with_context(|| format!("symbol {}", sym))?
        };
        unsafe { func(registrar) };
    }
    // Keep the DLL loaded
    std::mem::forget(lib);
    Ok(())
}

/// Discover every plugin DLL in `release_dir` and register it with
/// the Flutter engine identified by `engine`.  Each plugin DLL
/// (by its file‐stem name) runs *once* per engine.  
pub fn load_and_register_plugins(
    release_dir: &Path,
    engine: FlutterDesktopEngineRef,
    dll: Option<&Arc<FlutterDll>>,
) -> Result<()> {
    // Get (or create) this engine's seen‐set
    let mut map = registered_map().lock().unwrap();
    let seen = map.entry(engine as usize).or_default();

    // Find all candidate plugin DLLs
    let plugins = discover_plugins(release_dir)
        .with_context(|| format!("discovering plugins in `{}`", release_dir.display()))?;

    for (dll_path, symbols) in plugins {
        let plugin_name = dll_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // Skip if we've already registered that plugin into *this* engine
        if !seen.insert(plugin_name.clone()) {
            log::debug!(
                "[Plugin Loader] skipping `{}` (already registered)",
                plugin_name
            );
            continue;
        }

        log::info!(
            "[Plugin Loader] registering plugin `{}` from `{}`",
            plugin_name,
            dll_path.display()
        );

        // Grab the registrar from the Flutter engine
        let cname = std::ffi::CString::new(plugin_name).unwrap();
        let name_ptr = cname.as_ptr() as *const u8;
        let registrar: FlutterDesktopPluginRegistrarRef =
            unsafe { (dll.unwrap().FlutterDesktopEngineGetPluginRegistrar)(engine, name_ptr) };

        // Load & invoke registration routines
        load_and_register(&dll_path, &symbols, registrar)?;
    }

    Ok(())
}
