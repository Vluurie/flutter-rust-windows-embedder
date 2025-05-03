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
use crate::flutter_bindings::FlutterDesktopPluginRegistrar;

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

/// Load an empty cache every time.
fn load_cache(_release_dir: &Path) -> Result<PluginCache> {
    debug!("Plugin cache DISABLED: starting fresh every time");
    Ok(PluginCache::default())
}

/// Do nothing instead of writing a cache file.
fn save_cache(_release_dir: &Path, _cache: &PluginCache) -> Result<()> {
    debug!("Plugin cache DISABLED: not writing anything");
    Ok(())
}

/// Compute SHA-256 of a file.
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

/// Extract all export names ending with our registrar suffix.
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

/// Discover all DLLs, re-parse their symbols every time, and return them.
fn discover_plugins(release_dir: &Path) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let _cache = load_cache(release_dir)?;
    let mut updated: Vec<(PathBuf, Vec<String>)> = Vec::new();

    for entry in fs::read_dir(release_dir)
        .with_context(|| format!("reading directory {}", release_dir.display()))?
    {
        let dll_path = entry?.path();
        if dll_path.extension()
            .and_then(|e| e.to_str())
            .map_or(false, |e| e.eq_ignore_ascii_case("dll"))
        {
            let symbols = parse_registration_symbols(&dll_path)?;
            if symbols.is_empty() {
                continue;
            }
            info!("Discovered plugin `{}`", dll_path.display());
            updated.push((dll_path.clone(), symbols));
        }
    }

    Ok(updated)
}

/// Load each plugin DLL and invoke its `xxxRegisterWithRegistrar` functions.
pub fn load_and_register_plugins(
    release_dir: &Path,
    registrar: *mut FlutterDesktopPluginRegistrar,
) -> Result<()> {
    let plugins = discover_plugins(release_dir)
        .with_context(|| format!("discovering plugins in {}", release_dir.display()))?;

    if plugins.is_empty() {
        debug!("No Flutter plugins found in `{}`", release_dir.display());
        return Ok(());
    }

    // Keep libraries alive until end of function.
    let mut libs = Vec::with_capacity(plugins.len());

    for (dll_path, symbols) in plugins {
        info!("Loading plugin library `{}`", dll_path.display());
        let lib = unsafe {
            Library::new(&dll_path)
                .with_context(|| format!("loading {}", dll_path.display()))?
        };

        for sym in symbols {
            let cname = CString::new(sym.clone()).unwrap();
            let func: Symbol<unsafe extern "C" fn(*mut FlutterDesktopPluginRegistrar)> =
                unsafe {
                    lib.get(cname.as_bytes_with_nul())
                        .with_context(|| format!("symbol {}", sym))?
                };
            unsafe { func(registrar) };
            debug!("Registered symbol `{}`", sym);
        }

        libs.push(lib);
    }

    // Prevent drop so plugins stay registered.
    std::mem::forget(libs);

    Ok(())
}
