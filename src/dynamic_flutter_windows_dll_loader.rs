use crate::flutter_bindings as b;
use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use once_cell::sync::Lazy;
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{Arc, Mutex}};
use anyhow::anyhow;

#[derive(Debug)]
pub struct FlutterDll {
    _lib: &'static Library,

    pub FlutterDesktopEngineCreate: Symbol<
        'static,
        unsafe extern "C" fn(b::FlutterDesktopEngineProperties) -> b::FlutterDesktopEngineRef,
    >,
    pub FlutterDesktopEngineDestroy:
        Symbol<'static, unsafe extern "C" fn(b::FlutterDesktopEngineRef)>,
    pub FlutterDesktopEngineGetPluginRegistrar: Symbol<
        'static,
        unsafe extern "C" fn(
            b::FlutterDesktopEngineRef,
            *const u8,
        ) -> b::FlutterDesktopPluginRegistrarRef,
    >,
    pub FlutterDesktopEngineProcessExternalWindowMessage: Symbol<
    'static,
    unsafe extern "C" fn(
        b::FlutterDesktopEngineRef, 
        b::HWND,                    
        u32,                      
        b::WPARAM,                
        b::LPARAM,                 
        *mut b::LRESULT,            
    ) -> bool,
>,
    pub FlutterDesktopViewControllerCreate: Symbol<
        'static,
        unsafe extern "C" fn(
            i32,
            i32,
            b::FlutterDesktopEngineRef,
        ) -> b::FlutterDesktopViewControllerRef,
    >,
    pub FlutterDesktopViewControllerGetView: Symbol<
        'static,
        unsafe extern "C" fn(b::FlutterDesktopViewControllerRef) -> b::FlutterDesktopViewRef,
    >,
    pub FlutterDesktopViewControllerGetEngine: Symbol<
        'static,
        unsafe extern "C" fn(b::FlutterDesktopViewControllerRef) -> b::FlutterDesktopEngineRef,
    >,
    pub FlutterDesktopViewControllerHandleTopLevelWindowProc: Symbol<
        'static,
        unsafe extern "C" fn(
            b::FlutterDesktopViewControllerRef,
            b::HWND,                          
            u32,                              
            b::WPARAM,                        
            b::LPARAM,                       
            *mut b::LRESULT,                  
        ) -> bool,
    >,
    pub FlutterDesktopViewControllerDestroy:
        Symbol<'static, unsafe extern "C" fn(b::FlutterDesktopViewControllerRef)>,
    pub FlutterDesktopViewGetHWND:
        Symbol<'static, unsafe extern "C" fn(b::FlutterDesktopViewRef) -> b::HWND>,
}

static DLL_CACHE: Lazy<Mutex<HashMap<PathBuf, Arc<FlutterDll>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

impl FlutterDll {
    pub fn load(dir: Option<&Path>) -> Result<Self> {
        let dll_dir = if let Some(d) = dir {
            d.to_path_buf()
        } else {
            std::env::current_exe()
                .context("Failed to get current exe path")?
                .parent()
                .map(PathBuf::from)
                .context("Exe has no parent directory")?
        };

        let dll_path = dll_dir.join("flutter_windows.dll");
        let lib = unsafe { Library::new(&dll_path) }
            .with_context(|| format!("Failed to load {}", dll_path.display()))?;

        let lib_static: &'static Library = Box::leak(Box::new(lib));

        unsafe {
            let FlutterDesktopEngineCreate = lib_static
                .get(b"FlutterDesktopEngineCreate\0")
                .context("Missing symbol: FlutterDesktopEngineCreate")?;
            let FlutterDesktopEngineDestroy = lib_static
                .get(b"FlutterDesktopEngineDestroy\0")
                .context("Missing symbol: FlutterDesktopEngineDestroy")?;
            let FlutterDesktopEngineGetPluginRegistrar = lib_static
                .get(b"FlutterDesktopEngineGetPluginRegistrar\0")
                .context("Missing symbol: FlutterDesktopEngineGetPluginRegistrar")?;
            let FlutterDesktopEngineProcessExternalWindowMessage = lib_static
                .get(b"FlutterDesktopEngineProcessExternalWindowMessage\0")
                .context("Missing symbol: FlutterDesktopEngineProcessExternalWindowMessage")?;
            let FlutterDesktopViewControllerCreate = lib_static
                .get(b"FlutterDesktopViewControllerCreate\0")
                .context("Missing symbol: FlutterDesktopViewControllerCreate")?;
            let FlutterDesktopViewControllerGetView = lib_static
                .get(b"FlutterDesktopViewControllerGetView\0")
                .context("Missing symbol: FlutterDesktopViewControllerGetView")?;
            let FlutterDesktopViewControllerGetEngine = lib_static
                .get(b"FlutterDesktopViewControllerGetEngine\0")
                .context("Missing symbol: FlutterDesktopViewControllerGetEngine")?;
            let FlutterDesktopViewControllerHandleTopLevelWindowProc = lib_static
                .get(b"FlutterDesktopViewControllerHandleTopLevelWindowProc\0")
                .context("Missing symbol: FlutterDesktopViewControllerHandleTopLevelWindowProc")?;
            let FlutterDesktopViewControllerDestroy = lib_static
                .get(b"FlutterDesktopViewControllerDestroy\0")
                .context("Missing symbol: FlutterDesktopViewControllerDestroy")?;
            let FlutterDesktopViewGetHWND = lib_static
                .get(b"FlutterDesktopViewGetHWND\0")
                .context("Missing symbol: FlutterDesktopViewGetHWND")?;

            Ok(FlutterDll {
                _lib: lib_static,
                FlutterDesktopEngineCreate,
                FlutterDesktopEngineDestroy,
                FlutterDesktopEngineGetPluginRegistrar,
                FlutterDesktopEngineProcessExternalWindowMessage,
                FlutterDesktopViewControllerCreate,
                FlutterDesktopViewControllerGetView,
                FlutterDesktopViewControllerGetEngine,
                FlutterDesktopViewControllerHandleTopLevelWindowProc,
                FlutterDesktopViewControllerDestroy,
                FlutterDesktopViewGetHWND,
            })
        }
    }

    pub fn get_for(dir: Option<&Path>) -> Result<Arc<Self>> {
        let key = if let Some(d) = dir {
            d.to_path_buf()
        } else {
            std::env::current_exe()?
                .parent()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow!("Exe has no parent directory"))?
        };

        let mut cache = DLL_CACHE.lock().unwrap();
        if let Some(existing) = cache.get(&key) {
            return Ok(existing.clone());
        }

        let dll = FlutterDll::load(Some(&key))?;
        let arc = Arc::new(dll);
        cache.insert(key.clone(), arc.clone());
        Ok(arc)
    }
}
