use anyhow::{Context, Result, anyhow};
use libloading::{Library, Symbol};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    ffi::c_void,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::embedder as e;

#[derive(Debug)]
pub struct FlutterEngineDll {
    _lib: &'static Library,

    pub FlutterEngineRun: Symbol<
        'static,
        unsafe extern "C" fn(
            version: usize,
            config: *const e::FlutterRendererConfig,
            project_args: *const e::FlutterProjectArgs,
            user_data: *mut c_void,
            engine_out: *mut e::FlutterEngine,
        ) -> e::FlutterEngineResult,
    >,
    pub FlutterEngineShutdown:
        Symbol<'static, unsafe extern "C" fn(engine: e::FlutterEngine) -> e::FlutterEngineResult>,
    pub FlutterEngineInitialize: Symbol<
        'static,
        unsafe extern "C" fn(
            version: usize,
            config: *const e::FlutterRendererConfig,
            project_args: *const e::FlutterProjectArgs,
            user_data: *mut c_void,
            engine_out: *mut e::FlutterEngine,
        ) -> e::FlutterEngineResult,
    >,
    pub FlutterEngineRunInitialized:
        Symbol<'static, unsafe extern "C" fn(engine: e::FlutterEngine) -> e::FlutterEngineResult>,
    pub FlutterEngineDeinitialize:
        Symbol<'static, unsafe extern "C" fn(engine: e::FlutterEngine) -> e::FlutterEngineResult>,

    pub FlutterEngineSendWindowMetricsEvent: Symbol<
        'static,
        unsafe extern "C" fn(
            engine: e::FlutterEngine,
            event: *const e::FlutterWindowMetricsEvent,
        ) -> e::FlutterEngineResult,
    >,
    pub FlutterEngineSendPointerEvent: Symbol<
        'static,
        unsafe extern "C" fn(
            engine: e::FlutterEngine,
            events: *const e::FlutterPointerEvent,
            events_count: usize,
        ) -> e::FlutterEngineResult,
    >,
    pub FlutterEngineSendKeyEvent: Symbol<
        'static,
        unsafe extern "C" fn(
            engine: e::FlutterEngine,
            event: *const e::FlutterKeyEvent,
            key_handler: e::FlutterKeyEventCallback,
            user_data: *mut c_void,
        ) -> e::FlutterEngineResult,
    >,

    pub FlutterEngineSendPlatformMessage: Symbol<
        'static,
        unsafe extern "C" fn(
            engine: e::FlutterEngine,
            message: *const e::FlutterPlatformMessage,
        ) -> e::FlutterEngineResult,
    >,
    pub FlutterEngineSendPlatformMessageResponse: Symbol<
        'static,
        unsafe extern "C" fn(
            engine: e::FlutterEngine,
            handle: *const e::FlutterPlatformMessageResponseHandle,
            bytes: *const u8,
            bytes_length: usize,
        ) -> e::FlutterEngineResult,
    >,

    pub FlutterEngineRunTask: Symbol<
        'static,
        unsafe extern "C" fn(
            engine: e::FlutterEngine,
            task: *const e::FlutterTask,
        ) -> e::FlutterEngineResult,
    >,
    pub FlutterEngineScheduleFrame:
        Symbol<'static, unsafe extern "C" fn(engine: e::FlutterEngine) -> e::FlutterEngineResult>,
    pub FlutterEngineGetCurrentTime: Symbol<'static, unsafe extern "C" fn() -> u64>,

    pub FlutterEngineUpdateSemanticsEnabled: Symbol<
        'static,
        unsafe extern "C" fn(engine: e::FlutterEngine, enabled: bool) -> e::FlutterEngineResult,
    >,

    pub FlutterEngineCreateAOTData: Symbol<
        'static,
        unsafe extern "C" fn(
            source: *const e::FlutterEngineAOTDataSource,
            aot_data_out: *mut e::FlutterEngineAOTData,
        ) -> e::FlutterEngineResult,
    >,
    pub FlutterEngineOnVsync: Symbol<
        'static,
        unsafe extern "C" fn(
            engine: e::FlutterEngine,
            baton: isize,
            frame_start_time_nanos: u64,
            frame_target_time_nanos: u64,
        ) -> e::FlutterEngineResult,
    >,
}

static ENGINE_DLL_CACHE: Lazy<Mutex<HashMap<PathBuf, Arc<FlutterEngineDll>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

impl FlutterEngineDll {
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

        let dll_path = dll_dir.join("flutter_engine.dll");

        let lib_static: &'static Library = Box::leak(Box::new(
            unsafe { Library::new(&dll_path) }
                .with_context(|| format!("Failed to load {}", dll_path.display()))?,
        ));

        macro_rules! load_symbol {
            ($lib:expr, $name:expr) => {
                unsafe { $lib.get($name) }.with_context(|| {
                    format!(
                        "Missing symbol: {} in {}",
                        String::from_utf8_lossy($name),
                        dll_path.display()
                    )
                })
            };
        }

        Ok(FlutterEngineDll {
            _lib: lib_static,
            FlutterEngineRun: load_symbol!(lib_static, b"FlutterEngineRun\0")?,
            FlutterEngineShutdown: load_symbol!(lib_static, b"FlutterEngineShutdown\0")?,
            FlutterEngineInitialize: load_symbol!(lib_static, b"FlutterEngineInitialize\0")?,
            FlutterEngineRunInitialized: load_symbol!(
                lib_static,
                b"FlutterEngineRunInitialized\0"
            )?,
            FlutterEngineDeinitialize: load_symbol!(lib_static, b"FlutterEngineDeinitialize\0")?,
            FlutterEngineSendWindowMetricsEvent: load_symbol!(
                lib_static,
                b"FlutterEngineSendWindowMetricsEvent\0"
            )?,
            FlutterEngineSendPointerEvent: load_symbol!(
                lib_static,
                b"FlutterEngineSendPointerEvent\0"
            )?,
            FlutterEngineSendKeyEvent: load_symbol!(lib_static, b"FlutterEngineSendKeyEvent\0")?,
            FlutterEngineSendPlatformMessage: load_symbol!(
                lib_static,
                b"FlutterEngineSendPlatformMessage\0"
            )?,
            FlutterEngineSendPlatformMessageResponse: load_symbol!(
                lib_static,
                b"FlutterEngineSendPlatformMessageResponse\0"
            )?,
            FlutterEngineRunTask: load_symbol!(lib_static, b"FlutterEngineRunTask\0")?,
            FlutterEngineScheduleFrame: load_symbol!(lib_static, b"FlutterEngineScheduleFrame\0")?,
            FlutterEngineGetCurrentTime: load_symbol!(
                lib_static,
                b"FlutterEngineGetCurrentTime\0"
            )?,
            FlutterEngineUpdateSemanticsEnabled: load_symbol!(
                lib_static,
                b"FlutterEngineUpdateSemanticsEnabled\0"
            )?,
            FlutterEngineCreateAOTData: load_symbol!(lib_static, b"FlutterEngineCreateAOTData\0")?,
            FlutterEngineOnVsync: load_symbol!(lib_static, b"FlutterEngineOnVsync\0")?,
        })
    }

    pub fn get_for(dir: Option<&Path>) -> Result<Arc<Self>> {
        let key = if let Some(d) = dir {
            d.to_path_buf()
        } else {
            std::env::current_exe()
                .context("Failed to get current exe path for DLL key")?
                .parent()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow!("Exe has no parent directory for DLL key"))?
        };

        let mut cache = ENGINE_DLL_CACHE
            .lock()
            .map_err(|_| anyhow!("Failed to acquire DLL cache lock"))?;

        if let Some(existing) = cache.get(&key) {
            return Ok(existing.clone());
        }

        let dll = Self::load(Some(&key)).with_context(|| {
            format!(
                "Failed to load FlutterEngineDll from directory: {}",
                key.display()
            )
        })?;
        let arc_dll = Arc::new(dll);
        cache.insert(key.clone(), arc_dll.clone());
        Ok(arc_dll)
    }
}
