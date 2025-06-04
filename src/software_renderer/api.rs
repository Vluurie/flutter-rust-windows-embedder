use crate::embedder::{self as e, FlutterEngine};
use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::engine::update_flutter_window_metrics;
use crate::software_renderer::overlay::init as internal_embedder_init;

use crate::software_renderer::overlay::input::{handle_pointer_event, handle_set_cursor};
use crate::software_renderer::overlay::keyevents::handle_keyboard_event;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;
use crate::software_renderer::ticker::spawn::start_task_runner;
use crate::software_renderer::ticker::ticker::tick;

use log::{error, info, warn};
use std::path::PathBuf;
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
};
use windows::Win32::Graphics::Dxgi::IDXGISwapChain;

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

impl FlutterOverlay {
    /// Creates and initializes a new `FlutterOverlay` instance.
    ///
    /// This is the primary constructor for creating and setting up a Flutter overlay.
    /// It handles loading necessary DLLs, Flutter assets, initializing the Flutter engine,
    /// and preparing rendering resources.
    ///
    /// # Arguments
    /// * `name`: A unique name for this overlay instance.
    /// * `d3d11_device`: A reference to the host application's D3D11 device.
    /// * `swap_chain`: A reference to the DXGI swap chain.
    /// * `initial_width`: The initial width for the Flutter view.
    /// * `initial_height`: The initial height for the Flutter view.
    /// * `flutter_data_dir`: Path to the directory containing Flutter application's assets.
    /// * `dart_entrypoint_args`: Optional vector of strings for Dart VM entrypoint arguments.
    ///
    /// # Returns
    /// A `Result` containing a `Box<FlutterOverlay>` or a `FlutterEmbedderError`.
    pub fn create(
        name: String,
        d3d11_device: &ID3D11Device,
        swap_chain: &IDXGISwapChain,
        initial_width: u32,
        initial_height: u32,
        flutter_data_dir: PathBuf,
        dart_entrypoint_args: Option<Vec<String>>,
    ) -> Result<Box<Self>, FlutterEmbedderError> {
        info!(
            "[FlutterOverlay::create] Initializing Flutter Overlay '{}'. Data dir: {:?}",
            name, flutter_data_dir
        );

        let overlay_box = internal_embedder_init::init_overlay(
            name,
            Some(flutter_data_dir),
            d3d11_device,
            swap_chain,
            initial_width,
            initial_height,
            dart_entrypoint_args.as_deref(),
        );

        if overlay_box.engine.is_null() {
            error!(
                "[FlutterOverlay::create] Initialization failed: Engine handle is null after init."
            );
            return Err(FlutterEmbedderError::InitializationFailed(
                "Engine handle was null after internal init.".to_string(),
            ));
        }

        info!(
            "[FlutterOverlay::create] Flutter Overlay '{}' initialized successfully. Engine: {:?}",
            overlay_box.name, overlay_box.engine
        );
        Ok(overlay_box)
    }

    /// Returns the raw `FlutterEngine` pointer. **USE WITH CAUTION.**
    /// Prefer using methods on `FlutterOverlay` for interaction.
    pub fn get_engine_ptr(&self) -> FlutterEngine {
        self.engine
    }

    /// Handles resizing of the overlay. Updates dimensions, GPU resources,
    /// and informs the Flutter engine.
    pub fn handle_window_resize(&mut self, new_width: u32, new_height: u32, device: &ID3D11Device) {
        if self.width == new_width && self.height == new_height {
            info!(
                "[FlutterOverlay] Resize called but dimensions are same: W: {}, H: {}",
                new_width, new_height
            );
        }
        self.width = new_width;
        self.height = new_height;
        self.texture = create_texture(device, self.width, self.height);
        self.srv = create_srv(device, &self.texture);
        let new_buffer_size = (self.width as usize) * (self.height as usize) * 4;
        self.pixel_buffer.resize(new_buffer_size, 0);
        if !self.engine.is_null() {
            update_flutter_window_metrics(
                self.engine,
                self.width,
                self.height,
                self.engine_dll.clone(),
            );
        }
    }

    /// crate(INTERNAL) Starts the dedicated task runner thread for this overlay instance.
    /// Does nothing if the task runner is already running.
    pub(crate) fn start_task_runner(&mut self) {
        start_task_runner(self);
    }

    /// Performs per-frame updates, primarily uploading the pixel buffer to the GPU texture.
    pub fn tick(&self, context: &ID3D11DeviceContext) {
        // Geändert zu &self basierend auf vorheriger Logik
        tick(self, context);
    }

    /// Processes a Windows keyboard message for this overlay.
    /// # Returns
    /// `true` if Flutter handled the event, `false` otherwise.
    pub fn handle_keyboard_event(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
        // Der interne Handler gibt bool zurück, also geben wir es hier auch zurück.
        handle_keyboard_event(self, msg, wparam, lparam)
    }

    /// Processes a Windows mouse pointer message for this overlay.
    /// # Returns
    /// `true` if Flutter handled the event, `false` otherwise.
    pub fn handle_pointer_event(
        &self,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> bool {
        handle_pointer_event(self, hwnd, msg, wparam, lparam)
    }

    /// Handles a `WM_SETCURSOR` Windows message to set the cursor based on Flutter's request.
    /// # Returns
    /// `Some(LRESULT(1))` if Flutter handled the message and set the cursor, `None` otherwise.
    pub fn handle_set_cursor(
        &self,
        hwnd_from_wparam: HWND,
        lparam_from_message: LPARAM,
        main_app_hwnd: HWND,
    ) -> Option<LRESULT> {
        handle_set_cursor(self, hwnd_from_wparam, lparam_from_message, main_app_hwnd)
    }

    /// Notifies the Flutter engine that this overlay instance requests a new frame.
    /// Call this in your main loop to drive Flutter animations and UI updates.
    pub fn request_frame(&self) -> Result<(), FlutterEmbedderError> {
        if self.engine.is_null() {
            return Err(FlutterEmbedderError::EngineNotRunning);
        }
        unsafe {
            let result_code = (self.engine_dll.FlutterEngineScheduleFrame)(self.engine);
            if result_code == e::FlutterEngineResult_kSuccess {
                Ok(())
            } else {
                let err_msg = format!(
                    "FlutterEngineScheduleFrame FAILED for '{}': {:?}",
                    self.name, result_code
                );
                error!("[FlutterOverlay] {}", err_msg);
                Err(FlutterEmbedderError::OperationFailed(err_msg))
            }
        }
    }

    /// Retrieves the D3D11 Shader Resource View (SRV) for this overlay's texture.
    /// Used by the host application to render the Flutter UI.
    /// This clones the SRV (calls AddRef). The caller must Release it.
    pub fn get_texture_srv(&self) -> Result<ID3D11ShaderResourceView, FlutterEmbedderError> {
        unsafe {
            if self.srv.GetResource().is_err() {
                error!("[FlutterOverlay] SRV for '{}' is invalid.", self.name);
                return Err(FlutterEmbedderError::OperationFailed(format!(
                    "Texture SRV for overlay '{}' is not valid.",
                    self.name
                )));
            }
        }
        Ok(self.srv.clone())
    }

    /// Shuts down the Flutter engine associated with this overlay and cleans up all related resources.
    ///
    /// This method takes ownership of the `Box<FlutterOverlay>` instance to ensure that all
    /// resources, including the Flutter engine and any loaded libraries or textures,
    /// are properly deallocated. After calling this method, the overlay instance is consumed
    /// and can no longer be used.
    ///
    /// # Returns
    /// `Ok(())` on successful shutdown or if the overlay was already effectively shut down.
    /// Logs an error if `FlutterEngineShutdown` reports a failure but still attempts to complete resource cleanup.
    pub fn shutdown(self: Box<Self>) -> Result<(), FlutterEmbedderError> {
        info!(
            "[FlutterOverlay::shutdown] Shutting down Flutter Overlay for '{}', engine: {:?}",
            self.name, self.engine
        );

        if self.engine.is_null() {
            warn!(
                "[FlutterOverlay::shutdown] Shutdown attempted on an overlay with a null engine handle."
            );
            return Ok(());
        }

        if let Some(handle_arc) = self.task_runner_thread {
            if let Ok(handle) = Arc::try_unwrap(handle_arc) {
                info!(
                    "[FlutterOverlay::shutdown] Joining task runner thread for overlay '{}'...",
                    self.name
                );
                if let Err(e) = handle.join() {
                    error!(
                        "[FlutterOverlay::shutdown] Failed to join task runner thread for '{}': {:?}",
                        self.name, e
                    );
                } else {
                    info!(
                        "[FlutterOverlay::shutdown] Task runner thread for '{}' joined successfully.",
                        self.name
                    );
                }
            } else {
                warn!(
                    "[FlutterOverlay::shutdown] Task runner thread handle for '{}' still has multiple owners, cannot join directly here. Ensure graceful thread termination if necessary.",
                    self.name
                );
            }
        }

        unsafe {
            let result = (self.engine_dll.FlutterEngineShutdown)(self.engine);
            if result != e::FlutterEngineResult_kSuccess {
                let err_msg = format!(
                    "FlutterEngineShutdown failed for '{}': {:?}",
                    self.name, result
                );
                error!("[FlutterOverlay::shutdown] {}", err_msg);
                return Err(FlutterEmbedderError::OperationFailed(err_msg));
            } else {
                info!(
                    "[FlutterOverlay::shutdown] FlutterEngineShutdown successful for '{}'.",
                    self.name
                );
            }
        }
        Ok(())
    }
}
