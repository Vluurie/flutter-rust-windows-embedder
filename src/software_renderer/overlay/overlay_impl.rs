use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicPtr},
    },
    thread,
};

use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct3D11::{ID3D11ShaderResourceView, ID3D11Texture2D},
};

use crate::{
    bindings::embedder::{self, FlutterEngine},
    software_renderer::{
        d3d11_compositor::{compositor::D3D11Compositor, effects::EffectConfig},
        dynamic_flutter_engine_dll_loader::FlutterEngineDll,
        gl_renderer::gl_config::GLState,
        overlay::{semantics_handler::ProcessedSemanticsNode, textinput::ActiveTextInputState},
        ticker::task_scheduler::{
            SendableFlutterCustomTaskRunners, SendableFlutterTaskRunnerDescription, TaskQueueState,
            TaskRunnerContext,
        },
    },
};

pub static FLUTTER_LOG_TAG: &CStr =
    unsafe { CStr::from_bytes_with_nul_unchecked(b"rust_embedder\0") };

// A wrapper around the raw FlutterEngine pointer to make it Send + Sync.
// WARNING: This is only safe because we PROMISE to only use the pointer
// on the thread that the Flutter Engine is designed to run on.
#[derive(Debug, Copy, Clone)]
pub struct SendableFlutterEngine(pub FlutterEngine);

// By implementing these traits, we are making a manual guarantee to the compiler.
unsafe impl Send for SendableFlutterEngine {}
unsafe impl Sync for SendableFlutterEngine {}

#[derive(Debug)]
pub struct SendableGLState(pub Box<GLState>);

// By implementing these traits, we are making a manual guarantee to the compiler
// that we will only access the OpenGL state from the correct thread.
unsafe impl Send for SendableGLState {}
unsafe impl Sync for SendableGLState {}

impl PartialEq for SendableFlutterEngine {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<FlutterEngine> for SendableFlutterEngine {
    fn eq(&self, other: &FlutterEngine) -> bool {
        self.0 == *other
    }
}

impl std::fmt::Debug for GLState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GLState")
            .field("hdc", &self.hdc.0)
            .field("hglrc", &self.hglrc.0)
            .field("fbo_id", &self.fbo_id)
            .field("gl_texture_id", &self.gl_texture_id)
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SendHwnd(pub HWND);

unsafe impl Send for SendHwnd {}
unsafe impl Sync for SendHwnd {}

/// Represents a single Flutter overlay instance, managing its engine, rendering,
/// and various UI-related states.
/// Initialization is handled by `init_overlay`, which is responsible for correctly
/// populating all necessary fields.
#[repr(C)]
pub struct FlutterOverlay {
    /// The DirectX 11 texture resource on the GPU where the Flutter content is rendered.
    /// Other parts of the crate (e.g., the main game renderer) will use this to draw the overlay.
    /// **IMPORTANT: Must be a valid texture resource, initialized by `init_overlay`.**
    pub texture: ID3D11Texture2D,

    /// The DirectX 11 shader resource view for the `texture`.
    /// Used by shaders to sample from the overlay texture during rendering.
    /// **IMPORTANT: Must be a valid SRV, initialized by `init_overlay`.**
    pub srv: ID3D11ShaderResourceView,

    /// Current width of the overlay in pixels.
    /// Can be read by other parts of the crate for layout purposes.
    /// Updated by `handle_window_resize`. Must be > 0 for rendering.
    pub width: u32,

    /// Current height of the overlay in pixels.
    /// Can be read by other parts of the crate for layout purposes.
    /// Updated by `handle_window_resize`. Must be > 0 for rendering.
    pub height: u32,

    /// Overlay position X
    pub x: i32,
    /// Overlay position X
    pub y: i32,

    /// By default true and does nothing directly even when set to false
    /// Caller requires to call it when appropiated. Example like this:
    /// ```rust
    /// if !overlay_instance.is_visible() {
    ///     continue; // <-- ...we skip EVERYTHING below for this overlay.
    /// }
    ///
    /// overlay_instance.request_frame();
    /// update_interactive_widget_hover_state(overlay_instance);
    /// Self::paint_flutter_overlay(overlay_instance, painter, id);
    /// ```
    pub visible: bool,

    /// Configuration for shader effects applied to the overlay.
    pub effect_config: EffectConfig,

    /// A user-defined name for this overlay instance. Useful for identification,
    /// logging, or debugging purposes by any part of the crate.
    pub name: String,

    /// Atomic boolean indicating if the mouse cursor is currently hovering over an
    /// interactive widget (e.g., button, text field) within this overlay's semantics tree.
    /// Can be read by other parts of the crate (e.g., game input logic) to alter behavior.
    pub is_interactive_widget_hovered: AtomicBool,

    /// A boolean flag indicating if this specific overlay instance is running with
    /// debug assets (e.g., in JIT mode due to the absence of an AOT snapshot).
    /// Determined during `init_overlay` and can be read for diagnostic or conditional logic.
    pub is_debug_build: bool,

    /// Pointer to the native Flutter engine instance. Managed internally.
    /// Can be used to check for operations if the engine !is_null()
    /// **CRITICAL: Must be valid post-`init_overlay` for all operations.**
    pub engine: SendableFlutterEngine,

    /// The Direct3D 11 compositor responsible for rendering Flutter content to the texture.
    pub compositor: D3D11Compositor,

    pub gl_state: Option<SendableGLState>,
    pub gl_resource_state: Option<SendableGLState>,

    pub(crate) engine_atomic_ptr: Arc<AtomicPtr<embedder::_FlutterEngine>>,

    /// CPU-side buffer storing RGBA pixel data. Managed by Flutter rendering callbacks and `tick` method.
    pub(crate) pixel_buffer: Option<Vec<u8>>,

    /// The current cursor style requested by Flutter. Managed internally by `handle_set_cursor`
    /// and platform message callbacks.
    pub(crate) desired_cursor: Arc<Mutex<Option<String>>>,

    /// The Windows HWND this overlay is associated with. Set by `init_overlay`, used internally.
    pub(crate) windows_handler: SendHwnd,

    /// Shared reference to the loaded Flutter engine DLL. Managed internally.
    /// **CRITICAL INTERNAL: Must be valid post-`init_overlay`.**
    pub(crate) engine_dll: Arc<FlutterEngineDll>,

    /// State of the active text input field. Managed by text input callbacks and methods.
    pub(crate) text_input_state: Arc<Mutex<Option<ActiveTextInputState>>>,

    /// Instance-specific task queue. Managed by task runner and `post_task_callback`.
    pub(crate) task_queue_state: Arc<TaskQueueState>,

    /// Join handle for the task runner thread. Managed by `start_task_runner` and potentially `drop`.
    pub(crate) task_runner_thread: Option<Arc<thread::JoinHandle<()>>>,

    /// Tracks pressed mouse buttons for this overlay. Managed by `handle_pointer_event`.
    pub(crate) mouse_buttons_state: AtomicI32,

    /// Tracks if the `kAdd` pointer event was sent. Managed by `handle_pointer_event`.
    pub(crate) is_mouse_added: AtomicBool,

    /// Semantics tree data for this overlay. Managed by semantics callbacks and hover state updates.
    pub(crate) semantics_tree_data: Arc<Mutex<HashMap<i32, ProcessedSemanticsNode>>>,

    /// The Dart port used for sending messages directly to the Dart isolate.
    pub(crate) dart_send_port: Arc<AtomicI64>,

    // --- Crate-internal fields for FFI setup, primarily managed by `init_overlay`
    // and `build_project_args_and_strings`. These are implementation details.
    pub(crate) _assets_c: CString,
    pub(crate) _icu_c: CString,
    pub(crate) _engine_argv_cs: Vec<CString>,
    pub(crate) _dart_argv_cs: Vec<CString>,
    pub(crate) _aot_c: Option<CString>,
    pub(crate) _platform_runner_context: Option<Box<TaskRunnerContext>>,
    pub(crate) _platform_runner_description: Option<Box<SendableFlutterTaskRunnerDescription>>,
    pub(crate) _custom_task_runners_struct: Option<Box<SendableFlutterCustomTaskRunners>>,
}

impl Clone for FlutterOverlay {
    fn clone(&self) -> Self {
        Self {
            engine: self.engine,
            engine_atomic_ptr: self.engine_atomic_ptr.clone(),
            pixel_buffer: self.pixel_buffer.clone(),
            width: self.width,
            height: self.height,
            visible: true,
            effect_config: self.effect_config,
            x: self.x,
            y: self.y,
            texture: self.texture.clone(),
            srv: self.srv.clone(),
            compositor: self.compositor.clone(),
            gl_state: None,
            gl_resource_state: None,
            _platform_runner_context: None,
            _platform_runner_description: None,
            _custom_task_runners_struct: None,
            task_runner_thread: None,
            desired_cursor: self.desired_cursor.clone(),
            name: self.name.clone(),
            dart_send_port: self.dart_send_port.clone(),
            _assets_c: self._assets_c.clone(),
            _icu_c: self._icu_c.clone(),
            _engine_argv_cs: self._engine_argv_cs.clone(),
            _dart_argv_cs: self._dart_argv_cs.clone(),
            _aot_c: self._aot_c.clone(),
            engine_dll: self.engine_dll.clone(),
            task_queue_state: self.task_queue_state.clone(),

            text_input_state: self.text_input_state.clone(),
            mouse_buttons_state: AtomicI32::new(0),
            is_mouse_added: AtomicBool::new(false),
            semantics_tree_data: self.semantics_tree_data.clone(),
            is_interactive_widget_hovered: AtomicBool::new(false),
            windows_handler: self.windows_handler,
            is_debug_build: self.is_debug_build,
        }
    }
}

pub(crate) extern "C" fn on_present(
    user_data: *mut std::ffi::c_void,
    allocation: *const std::ffi::c_void,
    row_bytes_flutter: usize,
    height_flutter: usize,
) -> bool {
    crate::software_renderer::ticker::on_present(
        user_data,
        allocation,
        row_bytes_flutter,
        height_flutter,
    )
}
