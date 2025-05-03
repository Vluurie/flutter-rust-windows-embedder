// src/plugin_loader.rs

//! Dynamically discovers, caches, and registers Flutter plugins at runtime.
//!
//! 1. Scans the release directory for `.dll` files exporting
//!    `*RegisterWithRegistrar` symbols.  
//! 2. Caches plugin hashes and symbols to avoid redundant work.  
//! 3. Loads each plugin library and invokes its registration functions.

use anyhow::{Context, Result};
use bincode;
use goblin::Object;
use libloading::{Library, Symbol};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use crate::flutter_bindings::FlutterDesktopPluginRegistrar;

const CACHE_FILE: &str = "plugin_cache.bin";
const REG_SUFFIX: &str = "RegisterWithRegistrar"; // all Plugins have this suffix

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

/// Load the plugin cache from `release_dir/CACHE_FILE`,
/// or return an empty cache if missing.
fn load_cache(release_dir: &Path) -> Result<PluginCache> {
    let cache_path = release_dir.join(CACHE_FILE);
    if !cache_path.exists() {
        debug!("No plugin cache found at `{}`; starting fresh", cache_path.display());
        return Ok(PluginCache::default());
    }
    let mut buf = Vec::new();
    File::open(&cache_path)
        .with_context(|| format!("opening cache {}", cache_path.display()))?
        .read_to_end(&mut buf)
        .with_context(|| "reading cache bytes")?;
    let cache: PluginCache = bincode::deserialize(&buf)
        .with_context(|| "deserializing plugin cache")?;
    debug!("Loaded plugin cache with {} entries", cache.plugins.len());
    Ok(cache)
}

/// Persist the given cache to `release_dir/CACHE_FILE`.
fn save_cache(release_dir: &Path, cache: &PluginCache) -> Result<()> {
    let bytes = bincode::serialize(cache).with_context(|| "serializing plugin cache")?;
    let mut file = File::create(release_dir.join(CACHE_FILE))
        .with_context(|| format!("creating cache file in {}", release_dir.display()))?;
    file.write_all(&bytes)
        .with_context(|| "writing cache bytes")?;
    debug!("Plugin cache saved ({} entries)", cache.plugins.len());
    Ok(())
}

/// Compute the SHA-256 hash of the file at `path`.
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

/// HACK: Parse and return all export names ending with `REG_SUFFIX` from the PE at `path`.
fn parse_registration_symbols(path: &Path) -> Result<Vec<String>> {
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if let Object::PE(pe) = Object::parse(&data)? {
        // annotate the collected type so Rust knows we're collecting into Vec<String>
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
        // skip non-PE files silently
        Ok(Vec::new())
    }
}

/// Discover plugin DLLs in `release_dir`, updating the cache if files changed.
/// Returns a list of `(dll_path, symbols)` for each plugin to load.
fn discover_plugins(release_dir: &Path) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let mut cache = load_cache(release_dir)?;
    // holds (DLL path, registration symbols) for each plugin
    let mut updated: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut cache_changed = false;

    for entry in fs::read_dir(release_dir)
        .with_context(|| format!("reading directory {}", release_dir.display()))?
    {
        let dll_path = entry?.path();
        // only consider .dll files
        if dll_path.extension()
              .and_then(|e| e.to_str())
              .map_or(false, |e| e.eq_ignore_ascii_case("dll"))
        {
            // Determine relative path and compute current hash
            let rel_path = dll_path
                .strip_prefix(release_dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let current_hash = file_hash(&dll_path)?;

            // If unchanged, reuse cached symbols without reparsing
            if let Some(cached) = cache.plugins
                                           .iter()
                                           .find(|c| c.path == rel_path && c.hash == current_hash)
            {
                debug!("Cache hit for plugin `{}`", rel_path);
                updated.push((dll_path.clone(), cached.symbols.clone()));
                continue;
            }

            // NEW OR CHANGED: parse the registrar symbols
            let symbols: Vec<String> = parse_registration_symbols(&dll_path)?;
            if symbols.is_empty() {
                // not a plugin, skip
                continue;
            }

            info!("Discovered new/changed plugin `{}`", rel_path);
            updated.push((dll_path.clone(), symbols.clone()));

            // Update cache entry
            cache.plugins.retain(|c| c.path != rel_path);
            cache.plugins.push(CachedPlugin {
                path:    rel_path,
                hash:    current_hash,
                symbols: symbols.clone(),
            });
            cache_changed = true;
        }
    }

    if cache_changed {
        save_cache(release_dir, &cache)?;
    }

    Ok(updated)
}

/// Load each plugin DLL and invoke its `xxxRegisterWithRegistrar` exports.
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

    // Hold libraries alive until end of function
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

    // Prevent drop so plugins stay registered
    std::mem::forget(libs);
    Ok(())
}
