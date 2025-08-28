use crate::bindings::embedder::{
    self as e, FlutterEngine, FlutterEngineDartObject__bindgen_ty_1 as DartObjectUnion,
};
use crate::software_renderer::d3d11_compositor::primitive_3d_renderer::{PrimitiveType, Vertex3D};
use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::engine::update_flutter_window_metrics;
use crate::software_renderer::overlay::init::{self as internal_embedder_init};

use crate::software_renderer::overlay::input::{handle_pointer_event, handle_set_cursor};
use crate::software_renderer::overlay::keyevents::handle_keyboard_event;
use crate::software_renderer::overlay::overlay_impl::{FlutterOverlay, SendableHandle};
use crate::software_renderer::overlay::platform_message_callback::send_platform_message;
use crate::software_renderer::ticker::spawn::start_task_runner;
use crate::software_renderer::ticker::ticker::tick;
use log::{error, info, warn};
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::IDXGISwapChain;

#[derive(Debug, Clone, PartialEq)]
pub enum RendererType {
    Software,
    OpenGL,
}
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
        initial_x_pos: i32,
        initial_y_pos: i32,
        initial_width: u32,
        initial_height: u32,
        flutter_data_dir: PathBuf,
        dart_entrypoint_args: Option<Vec<String>>,
        engine_args_opt: Option<Vec<String>>,
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
            initial_x_pos,
            initial_y_pos,
            dart_entrypoint_args.as_deref(),
            engine_args_opt.as_deref(),
        );

        if overlay_box.engine.0.is_null() {
            error!(
                "[FlutterOverlay::create] Initialization failed: Engine handle is null after init."
            );
            return Err(FlutterEmbedderError::InitializationFailed(
                "Engine handle was null after internal init.".to_string(),
            ));
        }

        Ok(overlay_box)
    }

    /// Returns the raw `FlutterEngine` pointer. **USE WITH CAUTION.**
    /// Prefer using methods on `FlutterOverlay` for interaction.
    pub fn get_engine_ptr(&self) -> FlutterEngine {
        self.engine.0
    }

    pub fn handle_window_resize(
        &mut self,
        new_x: i32,
        new_y: i32,
        new_width: u32,
        new_height: u32,
        swap_chain: &IDXGISwapChain,
    ) {
        if self.width == new_width
            && self.height == new_height
            && self.x == new_x
            && self.y == new_y
        {
            return;
        }

        info!(
            "[handle_window_resize] Resizing overlay '{}' to {}x{}",
            self.name, new_width, new_height
        );

        self.width = new_width;
        self.height = new_height;
        self.x = new_x;
        self.y = new_y;

        let game_device = match unsafe { swap_chain.GetDevice::<ID3D11Device>() } {
            Ok(d) => d,
            Err(e) => {
                error!(
                    "[handle_window_resize] Failed to get device from swap chain: {}",
                    e
                );
                return;
            }
        };

        match self.renderer_type {
            RendererType::Software => {
                if let Some(pixel_buffer) = self.pixel_buffer.as_mut() {
                    self.texture = create_texture(&game_device, self.width, self.height);
                    self.srv = create_srv(&game_device, &self.texture);
                    let new_buffer_size = (self.width as usize) * (self.height as usize) * 4;
                    pixel_buffer.resize(new_buffer_size, 0);
                }
            }
            RendererType::OpenGL => {
                if let Some(angle_state) = self.angle_state.as_mut() {
                    info!("[handle_window_resize] Recreating ANGLE surface resources...");
                    match angle_state.0.recreate_resources(self.width, self.height) {
                        Ok((new_angle_texture, new_shared_handle)) => {
                            let new_game_texture: ID3D11Texture2D = unsafe {
                                let mut opened_resource_option: Option<ID3D11Texture2D> = None;
                                game_device
                                .OpenSharedResource(new_shared_handle, &mut opened_resource_option)
                                .expect("Failed to open new shared texture on game device after resize");
                                opened_resource_option
                                    .expect("Opened shared resource was null after resize")
                            };

                            self.texture = new_game_texture;
                            self.srv = create_srv(&game_device, &self.texture);
                            self.gl_internal_linear_texture = Some(new_angle_texture);
                            self.d3d11_shared_handle = Some(SendableHandle(new_shared_handle));
                            info!(
                                "[handle_window_resize] ANGLE surface resources recreated successfully."
                            );
                        }
                        Err(e) => {
                            error!(
                                "[handle_window_resize] Failed to recreate ANGLE resources: {}",
                                e
                            );
                        }
                    }
                } else {
                    warn!(
                        "[handle_window_resize] ANGLE state not found for OpenGL renderer during resize."
                    );
                }
            }
        }

        if !self.engine.0.is_null() {
            update_flutter_window_metrics(
                self.engine.0,
                self.x,
                self.y,
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

    /// Performs per-frame updates, preparing the GPU texture with the latest Flutter content.
    /// - For `Software` mode, it uploads pixel data from the CPU.
    /// - For `OpenGL` mode, it copies the frame from the shared ANGLE texture.
    pub fn tick(&self, context: &ID3D11DeviceContext) {
        if !self.visible || self.width == 0 || self.height == 0 {
            return;
        }

        match self.renderer_type {
            RendererType::Software => {
                tick(self, context);
            }
            RendererType::OpenGL => {
                if let Some(angle_texture) = &self.angle_shared_texture {
                    unsafe {
                        context.CopyResource(&self.texture, angle_texture);
                    }
                }
            }
        }
    }

    pub fn clear_all_queued_primitives(&mut self) {
        self.primitive_renderer.clear_all_primitives();
    }

    pub fn replace_queued_primitives_in_group(
        &mut self,
        group_id: &str,
        vertices: &[Vertex3D],
        topology: PrimitiveType,
    ) {
        match topology {
            PrimitiveType::Triangles => {
                self.primitive_renderer
                    .replace_primitives_in_group(group_id, vertices, &[]);
            }
            PrimitiveType::Lines => {
                self.primitive_renderer
                    .replace_primitives_in_group(group_id, &[], vertices);
            }
        }
    }

    pub fn clear_queued_primitives_in_group(&mut self, group_id: &str) {
        self.primitive_renderer.clear_primitives_in_group(group_id);
    }

    pub fn latch_queued_primitives(&mut self) {
        self.primitive_renderer.latch_buffers();
    }

    /// Processes a Windows keyboard message for this overlay.
    /// # Returns
    /// `true` if Flutter handled the event, `false` otherwise.
    pub fn handle_keyboard_event(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
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
        if self.engine.0.is_null() {
            return Err(FlutterEmbedderError::EngineNotRunning);
        }
        unsafe {
            let result_code = (self.engine_dll.FlutterEngineScheduleFrame)(self.engine.0);
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
        if self.engine.0.is_null() {
            warn!(
                "[FlutterOverlay::shutdown] Shutdown attempted on an overlay with a null engine handle."
            );
            return Ok(());
        }

        if let Some(handle_arc) = self.task_runner_thread {
            if let Ok(handle) = Arc::try_unwrap(handle_arc) {
                if let Err(e) = handle.join() {
                    error!(
                        "[FlutterOverlay::shutdown] Failed to join task runner thread for '{}': {:?}",
                        self.name, e
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
            let result = (self.engine_dll.FlutterEngineShutdown)(self.engine.0);
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

    /// Sets the screen-space position of the overlay's top-left corner.
    pub fn set_position(&mut self, new_x: i32, new_y: i32) {
        self.x = new_x;
        self.y = new_y;

        if !self.engine.0.is_null() {
            update_flutter_window_metrics(
                self.engine.0,
                self.x,
                self.y,
                self.width,
                self.height,
                self.engine_dll.clone(),
            );
        }
    }

    /// Get th widht and height of the overlay.
    pub fn get_dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Returns the current (x, y) position of the overlay.
    /// The counterpart to the `set_position` method you implemented.
    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn send_platform_message(
        &self,
        channel: &str,
        message: &[u8],
    ) -> Result<(), FlutterEmbedderError> {
        send_platform_message(self, channel, message)
    }

    /// Sets the visibility of the overlay.
    /// An invisible overlay will not be rendered and will not receive input.
    pub fn set_visibility(&mut self, is_visible: bool) {
        self.visible = is_visible;
    }

    /// Checks if the overlay is currently marked as visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Registers a custom handler for a platform channel.
    ///
    /// The handler takes the request bytes as an owned `Vec<u8>` and returns its
    /// response as a new `Vec<u8>`. This is a simpler, higher-level abstraction
    /// that handles memory allocation for the response automatically.
    ///
    /// # Arguments
    /// * `channel` - The name of the channel to listen on.
    /// * `handler` - A closure that takes `payload: Vec<u8>` and returns `Vec<u8>`.
    ///
    /// # Example
    /// ```rust, no_run
    /// my_overlay.register_channel_handler("my_game/get_player_state", |payload| {
    ///     let request_str = String::from_utf8_lossy(&payload);
    ///     println!("Request from Dart: {}", request_str);
    ///     
    ///     let state_json = r#"{"health": 100, "mana": 80}"#;
    ///     state_json.as_bytes().to_vec() // Return the response bytes
    /// });
    /// ```
    pub fn register_channel_handler<F>(&mut self, channel: &str, handler: F)
    where
        F: Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
    {
        match self.message_handlers.lock() {
            Ok(mut handlers) => {
                handlers.insert(channel.to_string(), Box::new(handler));
            }
            Err(poisoned) => {
                log::error!(
                    "Failed to acquire lock on message_handlers because it was poisoned: {}",
                    poisoned
                );
            }
        }
    }
    /// Triggers a "Hot Restart" for the running Flutter application.
    ///
    /// This works by sending a specific message on the "app/lifecycle" platform
    /// channel, which the Dart application must listen for.
    /// ```dart
    ///   const channel = BasicMessageChannel<String?>('app/lifecycle', StringCodec());
    ///   channel.setMessageHandler((String? message) async {
    ///     if (message == 'hot.restart') {
    ///       debugPrint("Hot restart command received from native code. Restarting...");
    ///       await ServicesBinding.instance.reassembleApplication();
    ///     }
    ///     return null;
    ///   });
    ///   ```
    pub fn hot_restart(&self) -> Result<(), FlutterEmbedderError> {
        info!(
            "[FlutterOverlay:'{}'] Sending 'hot.restart' command...",
            self.name
        );

        self.send_platform_message("app/lifecycle", "hot.restart".as_bytes())
    }

    /// Stores a Dart `SendPort` to enable native-to-Dart communication for this overlay.
    pub fn register_dart_port(&self, port: e::FlutterEngineDartPort) {
        info!(
            "[FlutterOverlay:'{}'] Registering Dart port: {}",
            self.name, port
        );
        self.dart_send_port.store(port, Ordering::SeqCst);
    }

    /// The internal dispatcher for sending a pre-constructed `FlutterEngineDartObject`
    /// to the Dart isolate.
    ///
    /// # Arguments
    ///
    /// * `object`: A reference to the FFI-compatible object to be sent.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the object was successfully posted.
    /// * `Err(FlutterEmbedderError)` if the engine is not running or if a Dart port
    ///   has not been registered via `register_dart_port`.
    ///
    /// # Safety
    ///
    /// The call to `FlutterEnginePostDartObject` is `unsafe` because it's a raw FFI
    /// call. This is considered safe in this context because:
    /// 1. The `FlutterEngineDll` loader ensures the function pointer is valid upon initialization.
    /// 2. We explicitly check that the `engine` handle is not null.
    /// 3. The `object` structure is built according to the C API's expectations.
    fn post_dart_object(
        &self,
        object: &e::FlutterEngineDartObject,
    ) -> Result<(), FlutterEmbedderError> {
        let port = self.dart_send_port.load(Ordering::SeqCst);
        if self.engine.0.is_null() {
            return Err(FlutterEmbedderError::EngineNotRunning);
        }
        if port == 0 {
            return Err(FlutterEmbedderError::OperationFailed(
                "Dart port not registered. Call `register_dart_port` first.".to_string(),
            ));
        }

        let result =
            unsafe { (self.engine_dll.FlutterEnginePostDartObject)(self.engine.0, port, object) };

        if result == e::FlutterEngineResult_kSuccess {
            Ok(())
        } else {
            let err_msg = format!("Failed to post Dart object with code: {:?}", result);
            error!("[FlutterOverlay:'{}'] {}", self.name, err_msg);
            Err(FlutterEmbedderError::OperationFailed(err_msg))
        }
    }

    /// Posts a boolean value to the Dart isolate.
    ///
    /// In Dart, this will be received as a `bool`.
    ///
    /// # Arguments
    ///
    /// * `value`: The `bool` value to send.
    ///
    /// # Returns
    ///
    /// * A `Result` indicating the success or failure of the operation. See `post_dart_object`.
    pub fn post_bool(&self, value: bool) -> Result<(), FlutterEmbedderError> {
        let obj = e::FlutterEngineDartObject {
            type_: e::FlutterEngineDartObjectType_kFlutterEngineDartObjectTypeBool,
            __bindgen_anon_1: DartObjectUnion { bool_value: value },
        };
        self.post_dart_object(&obj)
    }

    /// Posts a 64-bit integer to the Dart isolate.
    ///
    /// In Dart, this will be received as an `int`.
    ///
    /// # Arguments
    ///
    /// * `value`: The `i64` value to send.
    ///
    /// # Returns
    ///
    /// * A `Result` indicating the success or failure of the operation. See `post_dart_object`.
    pub fn post_i64(&self, value: i64) -> Result<(), FlutterEmbedderError> {
        let obj = e::FlutterEngineDartObject {
            type_: e::FlutterEngineDartObjectType_kFlutterEngineDartObjectTypeInt64,
            __bindgen_anon_1: DartObjectUnion { int64_value: value },
        };
        self.post_dart_object(&obj)
    }

    /// Posts a 64-bit floating-point number to the Dart isolate.
    ///
    /// In Dart, this will be received as a `double`.
    ///
    /// # Arguments
    ///
    /// * `value`: The `f64` value to send.
    ///
    /// # Returns
    ///
    /// * A `Result` indicating the success or failure of the operation. See `post_dart_object`.
    pub fn post_f64(&self, value: f64) -> Result<(), FlutterEmbedderError> {
        let obj = e::FlutterEngineDartObject {
            type_: e::FlutterEngineDartObjectType_kFlutterEngineDartObjectTypeDouble,
            __bindgen_anon_1: DartObjectUnion {
                double_value: value,
            },
        };
        self.post_dart_object(&obj)
    }

    /// Posts a UTF-8 string to the Dart isolate.
    ///
    /// This function handles the conversion from a Rust `&str` to a C-compatible,
    /// null-terminated string. The Flutter engine makes a copy of the string data,
    /// so the memory allocated for the C-string is safely freed when this function returns.
    ///
    /// In Dart, this will be received as a `String`.
    ///
    /// # Arguments
    ///
    /// * `value`: The string slice to send.
    ///
    /// # Errors
    ///
    /// Returns an error if the input string contains internal null `\0` bytes,
    /// as this is not permitted in C-style strings.
    pub fn post_string(&self, value: &str) -> Result<(), FlutterEmbedderError> {
        let c_string = match CString::new(value) {
            Ok(s) => s,
            Err(_) => {
                return Err(FlutterEmbedderError::OperationFailed(
                    "String contains null bytes.".to_string(),
                ));
            }
        };
        let obj = e::FlutterEngineDartObject {
            type_: e::FlutterEngineDartObjectType_kFlutterEngineDartObjectTypeString,
            __bindgen_anon_1: DartObjectUnion {
                string_value: c_string.as_ptr(),
            },
        };
        self.post_dart_object(&obj)
    }

    /// Posts a raw byte slice to the Dart isolate.
    ///
    /// This method is highly efficient for sending arbitrary binary data, such as
    /// serialized objects, file contents, or image data. The engine makes an internal
    /// copy of the buffer, so the caller retains ownership of the original slice.
    ///
    /// In Dart, this will be received as a `Uint8List`.
    ///
    /// # Arguments
    ///
    /// * `buffer`: The byte slice to send.
    ///
    /// # Returns
    ///
    /// * A `Result` indicating the success or failure of the operation. See `post_dart_object`.
    pub fn post_buffer(&self, buffer: &[u8]) -> Result<(), FlutterEmbedderError> {
        let dart_buffer = e::FlutterEngineDartBuffer {
            struct_size: std::mem::size_of::<e::FlutterEngineDartBuffer>(),
            user_data: std::ptr::null_mut(),
            buffer_collect_callback: None, // Lets the engine perform a copy.
            buffer: buffer.as_ptr() as *mut u8,
            buffer_size: buffer.len(),
        };

        let obj = e::FlutterEngineDartObject {
            type_: e::FlutterEngineDartObjectType_kFlutterEngineDartObjectTypeBuffer,
            __bindgen_anon_1: DartObjectUnion {
                buffer_value: &dart_buffer,
            },
        };
        self.post_dart_object(&obj)
    }
}
