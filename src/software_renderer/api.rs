// In flutter_rust_windows_embedder/src/software_renderer/api.rs

use std::path::PathBuf;
use std::ptr;

use log::{error, info, warn};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
};

use crate::embedder::{self as e};
use crate::software_renderer::overlay::init as internal_embedder_init;
use crate::software_renderer::overlay::input as internal_input_processor;
use crate::software_renderer::overlay::keyevents as internal_key_processor;
use crate::software_renderer::overlay::overlay_impl::{FLUTTER_OVERLAY_RAW_PTR, FlutterOverlay};

#[derive(Debug)]
pub enum FlutterEmbedderError {
    InitializationFailed(String),
    OperationFailed(String),
    EngineNotRunning,
    InvalidHandle,
}

impl std::fmt::Display for FlutterEmbedderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlutterEmbedderError::InitializationFailed(s) => {
                write!(f, "Flutter Initialization Failed: {}", s)
            }
            FlutterEmbedderError::OperationFailed(s) => {
                write!(f, "Flutter Operation Failed: {}", s)
            }
            FlutterEmbedderError::EngineNotRunning => {
                write!(f, "Flutter engine is not running or handle is null.")
            }
            FlutterEmbedderError::InvalidHandle => {
                write!(f, "Invalid Flutter overlay handle provided.")
            }
        }
    }
}
impl std::error::Error for FlutterEmbedderError {}

/// Initializes the Flutter software rendering overlay.
///
/// This function sets up the Flutter engine, loads necessary DLLs,
/// creates rendering textures, and prepares the overlay for use.
/// The returned `Box<FlutterOverlay>` is the handle to interact with the overlay.
///
/// # Arguments
/// * `d3d11_device`: A reference to the D3D11 device provided by the host application.
/// * `initial_width`: The initial width of the Flutter view (e.g., game window width).
/// * `initial_height`: The initial height of the Flutter view (e.g., game window height).
/// * `flutter_data_dir`: Path to the directory containing Flutter assets (`flutter_assets`, `icudtl.dat`).
///
/// # Returns
/// A `Result` containing a `Box<FlutterOverlay>` on success, or an `FlutterEmbedderError`.
pub fn initialize_overlay(
    d3d11_device: &ID3D11Device,
    initial_width: u32,
    initial_height: u32,
    flutter_data_dir: PathBuf,
) -> Result<Box<FlutterOverlay>, FlutterEmbedderError> {
    info!(
        "[EmbedderAPI] Initializing Flutter Overlay. Data dir: {:?}",
        flutter_data_dir
    );

    let overlay_box = internal_embedder_init::init_overlay(
        Some(flutter_data_dir),
        d3d11_device,
        initial_width,
        initial_height,
    );

    if overlay_box.engine.is_null() {
        error!("[EmbedderAPI] Initialization failed: Engine handle is null after init.");
        return Err(FlutterEmbedderError::InitializationFailed(
            "Engine handle was null after internal init.".to_string(),
        ));
    }

    info!(
        "[EmbedderAPI] Flutter Overlay initialized successfully. Engine: {:?}",
        overlay_box.engine
    );
    Ok(overlay_box)
}

/// Shuts down the Flutter engine and cleans up associated resources.
///
/// This function takes ownership of the `FlutterOverlay` box to ensure
/// proper deallocation of all resources, including the engine and DLL handles.
///
/// # Arguments
/// * `overlay_box`: The `Box<FlutterOverlay>` instance returned by `initialize_overlay`.
pub fn shutdown_overlay(overlay_box: Box<FlutterOverlay>) -> Result<(), FlutterEmbedderError> {
    info!(
        "[EmbedderAPI] Shutting down Flutter Overlay for engine: {:?}",
        overlay_box.engine
    );
    if overlay_box.engine.is_null() {
        warn!("[EmbedderAPI] Shutdown attempted on an overlay with a null engine handle.");
        return Ok(());
    }

    unsafe {
        let result = (overlay_box.engine_dll.FlutterEngineShutdown)(overlay_box.engine);
        if result != e::FlutterEngineResult_kSuccess {
            let err_msg = format!("FlutterEngineShutdown failed: {:?}", result);
            error!("[EmbedderAPI] {}", err_msg);
        } else {
            info!("[EmbedderAPI] FlutterEngineShutdown successful.");
        }

        if FLUTTER_OVERLAY_RAW_PTR
            == (&*overlay_box as *const FlutterOverlay as *mut FlutterOverlay)
        {
            FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut();
            info!("[EmbedderAPI] FLUTTER_OVERLAY_RAW_PTR cleared during shutdown.");
        }
    }

    Ok(())
}

/// Requests the Flutter engine to schedule and render a new frame.
///
/// This should be called by the host application in its render loop (e.g., once per game frame).
///
/// # Arguments
/// * `overlay`: A reference to the `FlutterOverlay` instance obtained from `initialize_overlay`.
pub fn request_frame(overlay: &FlutterOverlay) -> Result<(), FlutterEmbedderError> {
    if overlay.engine.is_null() {
        return Err(FlutterEmbedderError::EngineNotRunning);
    }
    unsafe {
        let result_code = (overlay.engine_dll.FlutterEngineScheduleFrame)(overlay.engine);
        if result_code == e::FlutterEngineResult_kSuccess {
            Ok(())
        } else {
            let err_msg = format!("FlutterEngineScheduleFrame FAILED: {:?}", result_code);
            error!("[EmbedderAPI] {}", err_msg);
            Err(FlutterEmbedderError::OperationFailed(err_msg))
        }
    }
}

/// Processes a Win32 pointer (mouse) message and forwards it to Flutter.
///
/// # Arguments
/// * `overlay`: A reference to the `FlutterOverlay` instance.
/// * `hwnd`: The window handle that received the message.
/// * `message_id`: The window message ID (e.g., `WM_MOUSEMOVE`).
/// * `wparam`: The `WPARAM` parameter of the window message.
/// * `lparam`: The `LPARAM` parameter of the window message.
/// * `flutter_can_process`: Boolean indicating if Flutter should currently process inputs.
///
/// # Returns
/// `true` if Flutter consumed the event, `false` otherwise.
pub fn forward_pointer_event(
    overlay: &FlutterOverlay,
    hwnd: HWND,
    message_id: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    flutter_can_process: bool,
) -> bool {
    if overlay.engine.is_null() {
        return false;
    }

    internal_input_processor::process_flutter_pointer_event_internal(
        overlay.engine,
        &overlay.engine_dll,
        hwnd,
        message_id,
        wparam,
        lparam,
        flutter_can_process,
    )
}

/// Processes a Win32 key message and forwards it to Flutter.
///
/// # Arguments
/// (Similar to `forward_pointer_event`)
///
/// # Returns
/// `true` if Flutter consumed the event, `false` otherwise.
pub fn forward_key_event(
    overlay: &FlutterOverlay,
    hwnd: HWND,
    message_id: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    flutter_can_process: bool,
) -> bool {
    if overlay.engine.is_null() {
        return false;
    }

    internal_key_processor::process_flutter_key_event_internal(
        overlay.engine,
        &overlay.engine_dll,
        hwnd,
        message_id,
        wparam,
        lparam,
        flutter_can_process,
    )
}

/// Handles a `WM_SETCURSOR` message to set the cursor based on Flutter's request.
///
/// # Arguments
/// * `_overlay`: A reference to the `FlutterOverlay` (currently unused by the internal logic
///             if `DESIRED_FLUTTER_CURSOR` is global, but passed for API consistency).
/// * `hwnd_from_wparam`: The HWND from `WM_SETCURSOR`'s WPARAM.
/// * `lparam_from_message`: The LPARAM from `WM_SETCURSOR` (contains hit-test code).
/// * `main_app_hwnd`: The HWND of your main application window hosting Flutter.
/// * `flutter_should_set_cursor`: Boolean indicating if Flutter should control the cursor.
///
/// # Returns
/// `Some(LRESULT(1))` if Flutter handled the cursor, `None` otherwise.
pub fn handle_set_cursor_event(
    _overlay: &FlutterOverlay,
    hwnd_from_wparam: HWND,
    lparam_from_message: LPARAM,
    main_app_hwnd: HWND,
    flutter_should_set_cursor: bool,
) -> Option<LRESULT> {
    internal_input_processor::handle_flutter_set_cursor(
        hwnd_from_wparam,
        lparam_from_message,
        main_app_hwnd,
        flutter_should_set_cursor,
    )
}

/// Retrieves the D3D11 Shader Resource View (SRV) for the Flutter overlay texture.
///
/// The host application can use this SRV to draw the Flutter UI onto its own surfaces.
/// The returned SRV is AddRef'd, and the caller should release it when done if it's stored
/// beyond the immediate scope, or ensure it's properly managed by egui/rendering frameworks.
///
/// # Arguments
/// * `overlay`: A reference to the `FlutterOverlay` instance.
///
/// # Returns
/// A `Result` containing the `ID3D11ShaderResourceView` or an `FlutterEmbedderError`.
pub fn get_texture_srv(
    overlay: &FlutterOverlay,
) -> Result<ID3D11ShaderResourceView, FlutterEmbedderError> {
    if unsafe { overlay.srv.GetResource().is_err() } {
        error!("[EmbedderAPI] SRV in FlutterOverlay is invalid or null.");
        return Err(FlutterEmbedderError::OperationFailed(
            "Texture SRV is not valid.".to_string(),
        ));
    }
    Ok(overlay.srv.clone())
}

/// If your rendering requires an explicit tick (e.g., for texture updates outside the main present callback).
/// Many software rendering setups might not need this if `present_software_surface` handles the update.
///
/// # Arguments
/// * `overlay`: A reference to the `FlutterOverlay` instance.
/// * `d3d_context`: The D3D11 immediate device context.
pub fn tick_render_target(
    overlay: &FlutterOverlay,
    d3d_context: &ID3D11DeviceContext,
) -> Result<(), FlutterEmbedderError> {
    if overlay.engine.is_null() {
        return Err(FlutterEmbedderError::EngineNotRunning);
    }

    FlutterOverlay::tick_global(d3d_context);
    Ok(())
}
