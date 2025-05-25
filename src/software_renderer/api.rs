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

/// Initializes the Flutter rendering overlay for embedding into a host application.
///
/// This function sets up the Flutter engine, loads necessary resources,
/// configures rendering targets, and prepares the overlay for interaction.
/// The returned `Box<FlutterOverlay>` serves as a handle to manage and interact
/// with the embedded Flutter instance.
///
/// # Arguments
/// * `d3d11_device`: A reference to the host application's D3D11 device, used for texture creation.
/// * `initial_width`: The initial width, in pixels, for the Flutter view, matching the host's render target dimensions.
/// * `initial_height`: The initial height, in pixels, for the Flutter view, matching the host's render target dimensions.
/// * `flutter_data_dir`: The file system path to the directory containing the Flutter application's assets
///   (e.g., `flutter_assets` directory and `icudtl.dat`).
///
/// # Returns
/// A `Result` containing a `Box<FlutterOverlay>` on successful initialization,
/// or a `FlutterEmbedderError` if initialization fails.
pub fn initialize_overlay(
    d3d11_device: &ID3D11Device,
    initial_width: u32,
    initial_height: u32,
    flutter_data_dir: PathBuf,
    dart_args_opt: Option<&[String]>,
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
        dart_args_opt
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

/// Shuts down the Flutter engine associated with the overlay and cleans up all related resources.
///
/// This function takes ownership of the `FlutterOverlay` instance to ensure that all
/// resources, including the Flutter engine and any loaded libraries or textures,
/// are properly deallocated. Call this when the Flutter overlay is no longer needed.
///
/// # Arguments
/// * `overlay_box`: The `Box<FlutterOverlay>` instance previously returned by `initialize_overlay`.
///
/// # Returns
/// `Ok(())` on successful shutdown or if the overlay was already effectively shut down.
/// Logs an error if `FlutterEngineShutdown` reports a failure but still attempts to complete resource cleanup.
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

/// Notifies the Flutter engine that the host application is requesting a new frame to be rendered.
///
/// Call this function as part of the host application's main rendering loop,
/// once per frame (e.g., in response to a `Present` call or a game engine update tick).
/// This allows Flutter to advance its animations, update its state, and render its UI.
///
/// # Arguments
/// * `overlay`: A reference to the `FlutterOverlay` instance for which to schedule a frame.
///
/// # Returns
/// `Ok(())` if the frame scheduling request was successfully sent to the Flutter engine.
/// Returns a `FlutterEmbedderError` if the engine is not running or if the request fails.
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

/// Forwards a Win32 pointer (mouse) message to the Flutter engine for processing.
///
/// This allows Flutter to react to mouse movements, clicks, and other pointer-related interactions.
/// Call this from the host application's window procedure (`WndProc`) when a relevant
/// pointer message is received.
///
/// # Arguments
/// * `overlay`: A reference to the active `FlutterOverlay` instance.
/// * `hwnd`: The window handle of the host window that received the pointer message.
/// * `message_id`: The Win32 message identifier (e.g., `WM_MOUSEMOVE`, `WM_LBUTTONDOWN`).
/// * `wparam`: The `WPARAM` parameter associated with the window message.
/// * `lparam`: The `LPARAM` parameter associated with the window message.
/// * `flutter_can_process`: A boolean indicating whether Flutter should currently process this input.
///   Set to `true` if Flutter has focus or should handle the input, `false` otherwise.
///
/// # Returns
/// `true` if the Flutter engine consumed or handled the pointer event.
/// `false` if Flutter did not handle the event, allowing the host application to process it further.
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

/// Forwards a Win32 keyboard message to the Flutter engine for processing.
///
/// This enables Flutter to respond to keyboard input, such as typing in text fields or triggering shortcuts.
/// Call this from the host application's window procedure (`WndProc`) when a relevant
/// keyboard message is received.
///
/// # Arguments
/// * `overlay`: A reference to the active `FlutterOverlay` instance.
/// * `hwnd`: The window handle of the host window that received the key message.
/// * `message_id`: The Win32 message identifier (e.g., `WM_KEYDOWN`, `WM_KEYUP`, `WM_CHAR`).
/// * `wparam`: The `WPARAM` parameter associated with the window message (often the virtual key code).
/// * `lparam`: The `LPARAM` parameter associated with the window message (contains repeat counts, scan codes, etc.).
/// * `flutter_can_process`: A boolean indicating whether Flutter should currently process this input.
///   Set to `true` if Flutter has focus or should handle the input, `false` otherwise.
///
/// # Returns
/// `true` if the Flutter engine consumed or handled the key event.
/// `false` if Flutter did not handle the event, allowing the host application to process it further.
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

/// Handles a `WM_SETCURSOR` window message to allow Flutter to influence the mouse cursor's appearance.
///
/// Call this function from the host application's window procedure (`WndProc`)
/// in response to a `WM_SETCURSOR` message. It allows Flutter to set the cursor
/// (e.g., to a text I-beam or a hand pointer) when the mouse is over relevant Flutter widgets.
/// The `_overlay` parameter is part of the API for consistency; cursor behavior is influenced by internal state.
///
/// # Arguments
/// * `_overlay`: A reference to the `FlutterOverlay` instance.
/// * `hwnd_from_wparam`: The window handle (`HWND`) from the `WPARAM` of the `WM_SETCURSOR` message.
/// * `lparam_from_message`: The `LPARAM` from the `WM_SETCURSOR` message, which contains hit-test information.
/// * `main_app_hwnd`: The `HWND` of the main application window that is hosting the Flutter overlay.
/// * `flutter_should_set_cursor`: A boolean indicating whether Flutter is currently in a state
///   where it should attempt to control the cursor appearance.
///
/// # Returns
/// `Some(LRESULT(1))` if Flutter handled the `WM_SETCURSOR` message and set the cursor.
/// `None` if Flutter did not handle the message, allowing the host application's default
/// `DefWindowProc` to process it.
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

/// Retrieves the D3D11 Shader Resource View (SRV) representing the Flutter UI's rendered output.
///
/// The host application uses this SRV to integrate the Flutter-rendered UI into its own
/// Direct3D11 rendering pipeline, for example, by drawing it onto a game object,
/// a UI panel, or as a full-screen overlay.
///
/// This function `AddRef`s the SRV. The caller is responsible for calling `Release`
/// on the SRV when it is no longer needed, unless managed by a rendering framework
/// that handles COM object lifecycles.
///
/// # Arguments
/// * `overlay`: A reference to the `FlutterOverlay` instance from which to get the texture.
///
/// # Returns
/// A `Result` containing the `ID3D11ShaderResourceView` on success.
/// Returns `FlutterEmbedderError` if the SRV is not available or invalid (e.g., engine not running,
/// or rendering setup issue).
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

/// Manually triggers an update ("tick") for the Flutter render target.
///
/// Call this function to ensure the Flutter texture is updated with the latest rendered content
/// from the Flutter engine when its update is not directly tied to the host's main presentation
/// loop (e.g., `IDXGISwapChain::Present`). This is necessary when Flutter renders to an
/// offscreen texture that requires explicit synchronization or processing by the host
/// application using the provided D3D11 device context.
///
/// If Flutter's rendering is already synchronized with `request_frame` and the host's
/// presentation mechanism, this call is not needed.
///
/// # Arguments
/// * `overlay`: A reference to the `FlutterOverlay` instance whose render target requires an update.
/// * `d3d_context`: The D3D11 immediate device context for GPU operations related to updating the render target.
///
/// # Returns
/// `Ok(())` if the tick operation was successful.
/// Returns `FlutterEmbedderError` if the engine is not running or an error occurs.
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