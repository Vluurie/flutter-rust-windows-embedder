use libloading::{Library, Symbol};
use log::{error, info, warn};
use once_cell::sync::OnceCell;
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

static AFTERMATH_ENABLED: AtomicBool = AtomicBool::new(false);

static AFTERMATH_LIB: OnceCell<AftermathLibrary> = OnceCell::new();

const GFSDK_AFTERMATH_VERSION_API_VERSION: u32 = 0x0000219;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AftermathResult {
    Success,

    NotAvailable,

    Fail,

    VersionMismatch,

    NotInitialized,

    InvalidAdapter,

    InvalidParameter,

    Unknown,

    ApiError,

    NvApiIncompatible,

    GettingContextDataWithNewCommandList,

    AlreadyInitialized,

    D3DDebugLayerNotCompatible,

    DriverInitFailed,

    DriverVersionNotSupported,

    OutOfMemory,

    FeatureNotEnabled,

    Disabled,
}

impl From<u32> for AftermathResult {
    fn from(value: u32) -> Self {
        match value {
            0x1 => AftermathResult::Success,
            0x2 => AftermathResult::NotAvailable,
            0xBAD00000 => AftermathResult::Fail,
            0xBAD00001 => AftermathResult::VersionMismatch,
            0xBAD00002 => AftermathResult::NotInitialized,
            0xBAD00003 => AftermathResult::InvalidAdapter,
            0xBAD00004 => AftermathResult::InvalidParameter,
            0xBAD00005 => AftermathResult::Unknown,
            0xBAD00006 => AftermathResult::ApiError,
            0xBAD00007 => AftermathResult::NvApiIncompatible,
            0xBAD00008 => AftermathResult::GettingContextDataWithNewCommandList,
            0xBAD00009 => AftermathResult::AlreadyInitialized,
            0xBAD0000A => AftermathResult::D3DDebugLayerNotCompatible,
            0xBAD0000B => AftermathResult::DriverInitFailed,
            0xBAD0000C => AftermathResult::DriverVersionNotSupported,
            0xBAD0000D => AftermathResult::OutOfMemory,
            0xBAD00010 => AftermathResult::FeatureNotEnabled,
            0xBAD00016 => AftermathResult::Disabled,
            _ => AftermathResult::Unknown,
        }
    }
}

#[repr(u32)]
pub enum GpuCrashDumpWatchedApiFlags {
    None = 0,
    DX = 0x1,
    Vulkan = 0x2,
}

#[repr(u32)]
pub enum GpuCrashDumpFeatureFlags {
    Default = 0x0,

    DeferDebugInfoCallbacks = 0x1,
}

#[repr(u32)]
pub enum FeatureFlags {
    None = 0x0,

    EnableMarkers = 0x1,

    EnableShaderErrorReporting = 0x8,
}

type GpuCrashDumpCallback = unsafe extern "C" fn(
    crash_dump_data: *const c_void,
    crash_dump_size: u32,
    user_data: *mut c_void,
);

type ShaderDebugInfoCallback = unsafe extern "C" fn(
    shader_debug_info: *const c_void,
    shader_debug_info_size: u32,
    user_data: *mut c_void,
);

type CrashDumpDescriptionCallback =
    unsafe extern "C" fn(add_value: AddValueFn, user_data: *mut c_void);

type ResolveMarkerCallback = unsafe extern "C" fn(
    marker_data: *const c_void,
    marker_data_size: u32,
    user_data: *mut c_void,
    resolved_marker_data: *mut *mut c_void,
    resolved_marker_data_size: *mut u32,
);

type AddValueFn = unsafe extern "C" fn(key: u32, value: *const i8);

type EnableGpuCrashDumpsFn = unsafe extern "C" fn(
    api_version: u32,
    watched_api_flags: u32,
    feature_flags: u32,
    crash_dump_callback: GpuCrashDumpCallback,
    shader_debug_info_callback: Option<ShaderDebugInfoCallback>,
    crash_dump_description_callback: Option<CrashDumpDescriptionCallback>,
    resolve_marker_callback: Option<ResolveMarkerCallback>,
    user_data: *mut c_void,
) -> u32;

type DisableGpuCrashDumpsFn = unsafe extern "C" fn() -> u32;

type GetCrashDumpStatusFn = unsafe extern "C" fn(out_status: *mut u32) -> u32;

type DX11InitializeFn =
    unsafe extern "C" fn(api_version: u32, feature_flags: u32, device: *mut c_void) -> u32;

struct AftermathLibrary {
    _lib: Library,
    enable_gpu_crash_dumps: EnableGpuCrashDumpsFn,
    disable_gpu_crash_dumps: DisableGpuCrashDumpsFn,
    get_crash_dump_status: GetCrashDumpStatusFn,
    dx11_initialize: DX11InitializeFn,
}

unsafe impl Send for AftermathLibrary {}
unsafe impl Sync for AftermathLibrary {}

impl AftermathLibrary {
    fn load(search_dir: Option<&Path>) -> Result<Self, String> {
        let dll_name = "GFSDK_Aftermath_Lib.x64.dll";

        let lib = if let Some(dir) = search_dir {
            let dll_path = dir.join(dll_name);
            if dll_path.exists() {
                unsafe { Library::new(&dll_path) }
            } else {
                unsafe { Library::new(dll_name) }
            }
        } else {
            unsafe { Library::new(dll_name) }
        };

        let lib = lib.map_err(|e| format!("Failed to load {}: {}", dll_name, e))?;

        unsafe {
            let enable_gpu_crash_dumps_sym: Symbol<EnableGpuCrashDumpsFn> = lib
                .get(b"GFSDK_Aftermath_EnableGpuCrashDumps\0")
                .map_err(|e| format!("Failed to get GFSDK_Aftermath_EnableGpuCrashDumps: {}", e))?;
            let enable_gpu_crash_dumps = *enable_gpu_crash_dumps_sym;

            let disable_gpu_crash_dumps_sym: Symbol<DisableGpuCrashDumpsFn> = lib
                .get(b"GFSDK_Aftermath_DisableGpuCrashDumps\0")
                .map_err(|e| {
                    format!("Failed to get GFSDK_Aftermath_DisableGpuCrashDumps: {}", e)
                })?;
            let disable_gpu_crash_dumps = *disable_gpu_crash_dumps_sym;

            let get_crash_dump_status_sym: Symbol<GetCrashDumpStatusFn> = lib
                .get(b"GFSDK_Aftermath_GetCrashDumpStatus\0")
                .map_err(|e| format!("Failed to get GFSDK_Aftermath_GetCrashDumpStatus: {}", e))?;
            let get_crash_dump_status = *get_crash_dump_status_sym;

            let dx11_initialize_sym: Symbol<DX11InitializeFn> = lib
                .get(b"GFSDK_Aftermath_DX11_Initialize\0")
                .map_err(|e| format!("Failed to get GFSDK_Aftermath_DX11_Initialize: {}", e))?;
            let dx11_initialize = *dx11_initialize_sym;

            Ok(AftermathLibrary {
                _lib: lib,
                enable_gpu_crash_dumps,
                disable_gpu_crash_dumps,
                get_crash_dump_status,
                dx11_initialize,
            })
        }
    }
}

unsafe extern "C" fn on_crash_dump(
    crash_dump_data: *const c_void,
    crash_dump_size: u32,
    _user_data: *mut c_void,
) {
    error!(
        "[Aftermath] GPU CRASH DETECTED! Dump size: {} bytes",
        crash_dump_size
    );

    if crash_dump_data.is_null() || crash_dump_size == 0 {
        error!("[Aftermath] Crash dump data is null or empty!");
        return;
    }

    let data = unsafe {
        std::slice::from_raw_parts(crash_dump_data as *const u8, crash_dump_size as usize)
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let filename = format!("gpu_crash_{}.nv-gpudmp", timestamp);

    let save_paths = [
        std::path::PathBuf::from(&filename),
        std::env::temp_dir().join(&filename),
    ];

    for path in &save_paths {
        match std::fs::write(path, data) {
            Ok(_) => {
                error!("[Aftermath] GPU crash dump saved to: {}", path.display());
                error!(
                    "[Aftermath] Open this file with NVIDIA Nsight Graphics for detailed analysis"
                );
                return;
            }
            Err(e) => {
                warn!(
                    "[Aftermath] Failed to save crash dump to {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }

    error!("[Aftermath] Failed to save crash dump to any location!");
}

unsafe extern "C" fn on_shader_debug_info(
    shader_debug_info: *const c_void,
    shader_debug_info_size: u32,
    _user_data: *mut c_void,
) {
    info!(
        "[Aftermath] Shader debug info available: {} bytes",
        shader_debug_info_size
    );

    if shader_debug_info.is_null() || shader_debug_info_size == 0 {
        return;
    }

    let data = unsafe {
        std::slice::from_raw_parts(
            shader_debug_info as *const u8,
            shader_debug_info_size as usize,
        )
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let filename = format!("shader_debug_{}.bin", timestamp);
    if let Err(e) = std::fs::write(&filename, data) {
        warn!("[Aftermath] Failed to save shader debug info: {}", e);
    }
}

unsafe extern "C" fn on_crash_dump_description(add_value: AddValueFn, _user_data: *mut c_void) {
    let app_name = b"Flutter_Embedder_ANGLE\0";

    unsafe { add_value(0, app_name.as_ptr() as *const i8) };
}

pub fn enable_gpu_crash_dumps(search_dir: Option<&Path>) -> Result<bool, String> {
    if AFTERMATH_ENABLED.load(Ordering::SeqCst) {
        info!("[Aftermath] Already enabled");
        return Ok(true);
    }

    info!("[Aftermath] Attempting to enable GPU crash dump collection...");

    let lib = match AFTERMATH_LIB.get_or_try_init(|| AftermathLibrary::load(search_dir)) {
        Ok(lib) => lib,
        Err(e) => {
            info!(
                "[Aftermath] SDK not available: {}. GPU crash debugging disabled.",
                e
            );
            return Ok(false);
        }
    };

    let result_code = unsafe {
        (lib.enable_gpu_crash_dumps)(
            GFSDK_AFTERMATH_VERSION_API_VERSION,
            GpuCrashDumpWatchedApiFlags::DX as u32,
            GpuCrashDumpFeatureFlags::Default as u32,
            on_crash_dump,
            Some(on_shader_debug_info),
            Some(on_crash_dump_description),
            None,
            std::ptr::null_mut(),
        )
    };

    let result = AftermathResult::from(result_code);

    match result {
        AftermathResult::Success => {
            AFTERMATH_ENABLED.store(true, Ordering::SeqCst);
            info!("[Aftermath] GPU crash dump collection ENABLED successfully!");
            info!("[Aftermath] Crash dumps will be saved as .nv-gpudmp files");
            Ok(true)
        }
        AftermathResult::NotAvailable => {
            info!(
                "[Aftermath] Not available on this system (non-NVIDIA GPU or unsupported driver)"
            );
            Ok(false)
        }
        AftermathResult::VersionMismatch => {
            warn!(
                "[Aftermath] API version mismatch. Code expects SDK Version 2.25 (0x{:03X}). \
                 Ensure GFSDK_Aftermath_Lib.x64.dll matches SDK 2025.4.0",
                GFSDK_AFTERMATH_VERSION_API_VERSION
            );
            Ok(false)
        }
        _ => {
            warn!(
                "[Aftermath] Failed to enable crash dumps: {:?} (raw code: {})",
                result, result_code
            );
            Ok(false)
        }
    }
}

pub fn initialize_d3d11_device(device: &ID3D11Device) -> Result<(), String> {
    if !AFTERMATH_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }

    let lib = match AFTERMATH_LIB.get() {
        Some(lib) => lib,
        None => return Ok(()),
    };

    info!("[Aftermath] Initializing D3D11 device for GPU crash monitoring...");

    use windows::core::Interface;
    let device_ptr = device.as_raw() as *mut c_void;

    let result = unsafe {
        (lib.dx11_initialize)(
            GFSDK_AFTERMATH_VERSION_API_VERSION,
            FeatureFlags::EnableMarkers as u32 | FeatureFlags::EnableShaderErrorReporting as u32,
            device_ptr,
        )
    };

    let result = AftermathResult::from(result);

    match result {
        AftermathResult::Success => {
            info!("[Aftermath] D3D11 device initialized successfully for crash monitoring");
            Ok(())
        }
        AftermathResult::NotAvailable => {
            info!("[Aftermath] D3D11 monitoring not available for this device");
            Ok(())
        }
        _ => Err(format!(
            "[Aftermath] Failed to initialize D3D11 device: {:?}",
            result
        )),
    }
}

pub fn disable_gpu_crash_dumps() {
    if !AFTERMATH_ENABLED.load(Ordering::SeqCst) {
        return;
    }

    if let Some(lib) = AFTERMATH_LIB.get() {
        unsafe {
            let result = (lib.disable_gpu_crash_dumps)();
            if AftermathResult::from(result) == AftermathResult::Success {
                info!("[Aftermath] GPU crash dump collection disabled");
            }
        }
    }

    AFTERMATH_ENABLED.store(false, Ordering::SeqCst);
}

pub fn is_enabled() -> bool {
    AFTERMATH_ENABLED.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashDumpStatus {
    Unknown = 0,
    NotStarted = 1,
    Collecting = 2,
    Finished = 3,
    InvokingCallback = 4,
}

impl From<u32> for CrashDumpStatus {
    fn from(value: u32) -> Self {
        match value {
            1 => CrashDumpStatus::NotStarted,
            2 => CrashDumpStatus::Collecting,
            3 => CrashDumpStatus::Finished,
            4 => CrashDumpStatus::InvokingCallback,
            _ => CrashDumpStatus::Unknown,
        }
    }
}

pub fn get_crash_dump_status() -> CrashDumpStatus {
    if !AFTERMATH_ENABLED.load(Ordering::SeqCst) {
        return CrashDumpStatus::NotStarted;
    }

    if let Some(lib) = AFTERMATH_LIB.get() {
        let mut status: u32 = 0;
        unsafe {
            let result = (lib.get_crash_dump_status)(&mut status);
            if AftermathResult::from(result) == AftermathResult::Success {
                return CrashDumpStatus::from(status);
            }
        }
    }

    CrashDumpStatus::Unknown
}

pub fn wait_for_crash_dump(max_wait_ms: u32) -> bool {
    if !AFTERMATH_ENABLED.load(Ordering::SeqCst) {
        return true;
    }

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(max_wait_ms as u64);

    loop {
        let status = get_crash_dump_status();

        match status {
            CrashDumpStatus::Finished | CrashDumpStatus::NotStarted => {
                return true;
            }
            CrashDumpStatus::Collecting | CrashDumpStatus::InvokingCallback => {
                if start.elapsed() >= timeout {
                    warn!(
                        "[Aftermath] Timeout waiting for crash dump (status: {:?})",
                        status
                    );
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            CrashDumpStatus::Unknown => {
                return true;
            }
        }
    }
}
