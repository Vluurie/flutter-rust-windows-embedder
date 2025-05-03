// src/plugin_loader.rs

use anyhow::{Context, Result};
use goblin::Object;
use libloading::{Library, Symbol};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::flutter_bindings::{
    FlutterDesktopEngineRef,
    FlutterDesktopPluginRegistrarRef,
    FlutterDesktopEngineGetPluginRegistrar,
};

const REG_SUFFIX: &str = "RegisterWithRegistrar";

/// Scan the directory for all DLLs exporting `*RegisterWithRegistrar`.
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

/// Load one DLL and invoke each `xxxRegisterWithRegistrar` symbol.
fn load_and_register(
    dll: &Path,
    symbols: &[String],
    registrar: FlutterDesktopPluginRegistrarRef,
) -> Result<()> {
    let lib = unsafe { Library::new(dll).with_context(|| format!("loading {}", dll.display()))? };
    for sym in symbols {
        let cname = CString::new(sym.as_str()).unwrap();
        let func: Symbol<unsafe extern "C" fn(FlutterDesktopPluginRegistrarRef)> =
            unsafe { lib.get(cname.as_bytes_with_nul()).with_context(|| format!("symbol {}", sym))? };
        unsafe { func(registrar) };
    }
    // keep the library alive for the life of the process
    std::mem::forget(lib);
    Ok(())
}

/// Discover every plugin DLL and register it against the engine registrar.
pub fn load_and_register_plugins(
    release_dir: &Path,
    engine: FlutterDesktopEngineRef,
) -> Result<()> {
    let plugins = discover_plugins(release_dir)
        .with_context(|| format!("discovering plugins in {}", release_dir.display()))?;
    for (dll_path, symbols) in plugins {
        // derive plugin name from the file stem
        let plugin_name = dll_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let c_name = CString::new(plugin_name)?;
        let registrar = unsafe {
            FlutterDesktopEngineGetPluginRegistrar(engine, c_name.as_ptr())
        };
        load_and_register(&dll_path, &symbols, registrar)?;
    }
    Ok(())
}
