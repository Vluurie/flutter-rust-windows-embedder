// src/plugin_loader.rs

//! Dynamically discovers and registers Flutter plugins at runtime,
//! without using an on-disk cache.

use anyhow::{Context, Result};
use goblin::Object;
use libloading::{Library, Symbol};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
use crate::flutter_bindings::{self, FlutterDesktopEngineRef, FlutterDesktopPluginRegistrar};

const REG_SUFFIX: &str = "RegisterWithRegistrar";

#[derive(Serialize, Deserialize)]
struct CachedPlugin {
    path: String,
    hash: Vec<u8>,
    symbols: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct PluginCache {
    plugins: Vec<CachedPlugin>,
}

/// Always start with an empty cache.
fn load_cache(_release_dir: &Path) -> Result<PluginCache> {
    debug!("Plugin cache DISABLED: starting fresh every time");
    Ok(PluginCache::default())
}

/// No-op save.
fn save_cache(_release_dir: &Path, _cache: &PluginCache) -> Result<()> {
    debug!("Plugin cache DISABLED: not writing anything");
    Ok(())
}

/// SHA-256 for completeness (unused when cache disabled).
fn file_hash(path: &Path) -> Result<Vec<u8>> {
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_vec())
}

/// Find all `*RegisterWithRegistrar` exports in a PE DLL.
fn parse_registration_symbols(path: &Path) -> Result<Vec<String>> {
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if let Object::PE(pe) = Object::parse(&data)? {
        let symbols: Vec<String> = pe
            .exports
            .iter()
            .filter_map(|e| e.name)
            .filter(|n| n.ends_with(REG_SUFFIX))
            .map(|s| s.to_string())
            .collect();
        if !symbols.is_empty() {
            debug!(
                "Found {} registrar symbol(s) in `{}`",
                symbols.len(),
                path.display()
            );
        }
        Ok(symbols)
    } else {
        Ok(Vec::new())
    }
}

/// Scan the directory for all plugin DLLs and their registrar symbols.
fn discover_plugins(release_dir: &Path) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let _ = load_cache(release_dir)?;
    let mut out = Vec::new();

    for entry in fs::read_dir(release_dir)
        .with_context(|| format!("reading directory {}", release_dir.display()))?
    {
        let dll = entry?.path();
        if dll.extension()
            .and_then(|e| e.to_str())
            .map_or(false, |e| e.eq_ignore_ascii_case("dll"))
        {
            let syms = parse_registration_symbols(&dll)?;
            if !syms.is_empty() {
                info!("Discovered plugin `{}`", dll.display());
                out.push((dll, syms));
            }
        }
    }

    Ok(out)
}

/// Primitive: load one DLL and invoke each `xxxRegisterWithRegistrar`.
fn load_and_register(
    dll: &Path,
    symbols: &[String],
    registrar: *mut FlutterDesktopPluginRegistrar,
) -> Result<()> {
    info!("Loading plugin library `{}`", dll.display());
    let lib = unsafe { Library::new(dll).with_context(|| format!("loading {}", dll.display()))? };
    for sym in symbols {
        let cname = CString::new(sym.clone()).unwrap();
        let f: Symbol<unsafe extern "C" fn(*mut FlutterDesktopPluginRegistrar)> =
            unsafe { lib.get(cname.as_bytes_with_nul()).with_context(|| format!("symbol {}", sym))? };
        unsafe { f(registrar) };
        debug!("Registered symbol `{}`", sym);
    }
    std::mem::forget(lib);
    Ok(())
}

/// Which plugins need the *view*?  (i.e. texture APIs)
fn is_view_plugin(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map_or(false, |n| n.contains("window_manager") || n.contains("screen_retriever"))
}

/// Phase 1: register engine-only plugins (channels, etc.).
pub fn load_engine_plugins(
    release_dir: &Path,
    engine: FlutterDesktopEngineRef,
) -> Result<()> {
    let list = discover_plugins(release_dir)
        .with_context(|| format!("discovering plugins in {}", release_dir.display()))?;
    for (dll, syms) in list.iter().filter(|(dll, _)| !is_view_plugin(dll)) {
        let reg = unsafe {
            flutter_bindings::FlutterDesktopEngineGetPluginRegistrar(engine, std::ptr::null())
        };
        load_and_register(dll, syms, reg)?;
    }
    Ok(())
}

/// Phase 2: register view-level plugins (textures, embedding helpers).
pub fn load_view_plugins(
    release_dir: &Path,
    engine: FlutterDesktopEngineRef,
) -> Result<()> {
    let list = discover_plugins(release_dir)
        .with_context(|| format!("discovering plugins in {}", release_dir.display()))?;
    for (dll, syms) in list.iter().filter(|(dll, _)| is_view_plugin(dll)) {
        let reg = unsafe {
            flutter_bindings::FlutterDesktopEngineGetPluginRegistrar(engine, std::ptr::null())
        };
        load_and_register(dll, syms, reg)?;
    }
    Ok(())
}
