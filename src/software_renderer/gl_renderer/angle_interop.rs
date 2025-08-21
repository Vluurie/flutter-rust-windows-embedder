use crate::bindings::embedder;

use crate::software_renderer::overlay::d3d::create_shared_texture_and_get_handle;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;

use libloading::{Library, Symbol};
use log::{debug, error, info};
use once_cell::sync::OnceCell;
use std::ffi::{CString, c_void};
use std::path::{Path, PathBuf};
use std::thread::current;
use std::{mem, ptr};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::core::Interface;

// EGL and OpenGL constants used for ANGLE configuration and operation.
pub const EGL_DEFAULT_DISPLAY: *mut c_void = 0 as *mut c_void;
pub const EGL_NO_CONTEXT: *mut c_void = 0 as *mut c_void;
pub const EGL_NO_DISPLAY: *mut c_void = 0 as *mut c_void;
pub const EGL_NO_SURFACE: *mut c_void = 0 as *mut c_void;
pub const EGL_TRUE: i32 = 1;
pub const EGL_NONE: i32 = 0x3038;
pub const EGL_SUCCESS: i32 = 0x3000;
pub const EGL_WIDTH: i32 = 0x3057;
pub const EGL_HEIGHT: i32 = 0x3056;
pub const EGL_D3D11_TEXTURE_ANGLE: i32 = 0x3484;
pub const GL_BGRA_EXT: i32 = 0x87;

pub const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
pub const EGL_SURFACE_TYPE: i32 = 0x3033;
pub const EGL_PBUFFER_BIT: i32 = 0x0001;
pub const EGL_RENDERABLE_TYPE: i32 = 0x3040;
pub const EGL_OPENGL_ES2_BIT: i32 = 0x0004;
pub const EGL_RED_SIZE: i32 = 0x3024;
pub const EGL_GREEN_SIZE: i32 = 0x3023;
pub const EGL_BLUE_SIZE: i32 = 0x3022;
pub const EGL_ALPHA_SIZE: i32 = 0x3021;
pub const EGL_DEPTH_SIZE: i32 = 0x3025;
pub const EGL_STENCIL_SIZE: i32 = 0x3026;

pub const EGL_PLATFORM_ANGLE_ANGLE: i32 = 0x3202;
pub const EGL_PLATFORM_ANGLE_TYPE_ANGLE: i32 = 0x3203;
pub const EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE: i32 = 0x3208;
pub const EGL_PLATFORM_ANGLE_ENABLE_AUTOMATIC_TRIM_ANGLE: i32 = 0x320F;
pub const EGL_EXPERIMENTAL_PRESENT_PATH_ANGLE: i32 = 0x33A4;
pub const EGL_EXPERIMENTAL_PRESENT_PATH_FAST_ANGLE: i32 = 0x33A9;

pub const EGL_DEVICE_EXT: i32 = 0x322C;
pub const EGL_D3D11_DEVICE_ANGLE: i32 = 0x33A1;
pub const EGL_D3D_TEXTURE_ANGLE: i32 = 0x33A3;
pub const EGL_TEXTURE_INTERNAL_FORMAT_ANGLE: i32 = 0x345D;

// Type aliases for EGL/GL function pointers for FFI.
type EglGetProcAddress = unsafe extern "C" fn(*const i8) -> *mut c_void;
type EGLBoolean = i32;
type EglGetPlatformDisplayEXT = unsafe extern "C" fn(i32, *mut c_void, *const i32) -> *mut c_void;
type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> bool;
type EglChooseConfig =
    unsafe extern "C" fn(*mut c_void, *const i32, *mut *mut c_void, i32, *mut i32) -> bool;
type EglCreateContext =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglMakeCurrent =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> i32;
type EglDestroyContext = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
type EglTerminate = unsafe extern "C" fn(*mut c_void) -> bool;
type EglGetError = unsafe extern "C" fn() -> i32;
type EglQueryDisplayAttribEXT = unsafe extern "C" fn(*mut c_void, i32, *mut isize) -> bool;
type EglQueryDeviceAttribEXT = unsafe extern "C" fn(*mut c_void, i32, *mut isize) -> bool;
type GlFinish = unsafe extern "C" fn();
type EglCreatePbufferFromClientBuffer =
    unsafe extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglDestroySurface = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;

///
/// Converts a raw EGL error code into a human-readable string literal.
///
fn egl_error_to_string(error_code: i32) -> &'static str {
    match error_code {
        0x3000 => "EGL_SUCCESS",
        0x3001 => "EGL_NOT_INITIALIZED",
        0x3002 => "EGL_BAD_ACCESS",
        0x3003 => "EGL_BAD_ALLOC",
        0x3004 => "EGL_BAD_ATTRIBUTE",
        0x3005 => "EGL_BAD_CONFIG",
        0x3006 => "EGL_BAD_CONTEXT",
        0x3007 => "EGL_BAD_CURRENT_SURFACE",
        0x3008 => "EGL_BAD_DISPLAY",
        0x3009 => "EGL_BAD_MATCH",
        0x300A => "EGL_BAD_NATIVE_PIXMAP",
        0x300B => "EGL_BAD_NATIVE_WINDOW",
        0x300C => "EGL_BAD_PARAMETER",
        0x300D => "EGL_BAD_SURFACE",
        0x300E => "EGL_CONTEXT_LOST",
        _ => "Unknown EGL error",
    }
}

///
/// Retrieves the last EGL error using the provided function pointer and logs it
/// to the error channel if an error has occurred.
///
fn log_egl_error(func: &str, line: u32, egl_get_error_fn: EglGetError) {
    let code = unsafe { egl_get_error_fn() };
    if code != EGL_SUCCESS {
        error!(
            "[ANGLE DEBUG] EGL Error in {}:{} -> {} ({:#X})",
            func,
            line,
            egl_error_to_string(code),
            code
        );
    }
}

///
/// Global, thread-safe, lazily-initialized container for the shared EGL state.
/// This ensures that ANGLE libraries are loaded exactly once per process.
///
static SHARED_EGL: OnceCell<SharedEglState> = OnceCell::new();

///
/// Holds the process-wide, shared handles to the loaded ANGLE libraries (`libEGL.dll`,
/// `libGLESv2.dll`) and the core `eglGetProcAddress` function pointer.
/// This struct is initialized once by `get_or_init_shared_egl` and then shared
/// across all `AngleInteropState` instances to ensure consistency.
///
struct SharedEglState {
    libegl: Library,
    _libgles: Library,
    egl_get_proc_address: EglGetProcAddress,
}

///
/// Manages the EGL context, display, surfaces, and the ANGLE-created D3D11 device
/// for a single Flutter overlay instance. It relies on a globally shared `SharedEglState`
/// for the underlying library handles and function pointers.
///
#[derive(Debug)]
pub struct AngleInteropState {
    pub egl_make_current: EglMakeCurrent,
    egl_get_error: EglGetError,
    egl_destroy_context: EglDestroyContext,
    egl_terminate: EglTerminate,
    egl_create_context: EglCreateContext,
    gl_finish: GlFinish,
    egl_create_pbuffer_from_client_buffer: EglCreatePbufferFromClientBuffer,
    egl_destroy_surface: EglDestroySurface,
    pub display: *mut c_void,
    pub context: *mut c_void,
    pub resource_context: *mut c_void,
    pub angle_d3d11_device: ID3D11Device,
    config: *mut c_void,
    pub pbuffer_surface: *mut c_void,
    main_thread_id: Option<std::thread::ThreadId>,
    resource_thread_id: Option<std::thread::ThreadId>,
}

impl AngleInteropState {
    ///
    /// Creates and initializes a new ANGLE interop context for a Flutter overlay.
    ///
    /// This function orchestrates the entire ANGLE setup, including obtaining the shared
    /// EGL state, creating an EGL display, initializing EGL, querying for a D3D11
    /// device created by ANGLE, and preparing EGL contexts and configurations.
    ///
    /// # Arguments
    ///
    /// * `engine_dir`: An optional path to the directory containing `libEGL.dll` and
    ///   `libGLESv2.dll`. This path is only used during the very first initialization
    ///   of the shared EGL state within the process. Subsequent calls will ignore this
    ///   parameter and reuse the existing shared state.
    ///
    /// # Returns
    ///
    /// A `Result` containing the fully initialized `AngleInteropState` on success,
    /// or an error string on failure.
    ///
    pub fn new(engine_dir: Option<&Path>) -> Result<Box<Self>, String> {
        unsafe {
            info!("[AngleInterop] Initializing ANGLE and letting it create a D3D11 device...");

            let shared_egl = get_or_init_shared_egl(engine_dir)?;

            let get_proc = |name: &str| -> *mut c_void {
                let c_name = CString::new(name).unwrap();
                (shared_egl.egl_get_proc_address)(c_name.as_ptr())
            };

            let get_proc_assert = |name: &str| {
                let ptr = get_proc(name);
                assert!(!ptr.is_null(), "Failed to load {}", name);
                ptr
            };

            let proc_ptr = get_proc("eglGetPlatformDisplayEXT");

            if proc_ptr.is_null() {
                return Err("eglGetPlatformDisplayEXT not available".to_string());
            }

            let egl_get_platform_display_ext: EglGetPlatformDisplayEXT = mem::transmute(proc_ptr);
            let egl_initialize: EglInitialize = mem::transmute(get_proc("eglInitialize"));
            let egl_get_error: EglGetError = mem::transmute(get_proc("eglGetError"));

            let display_attributes: [i32; 7] = [
                EGL_PLATFORM_ANGLE_TYPE_ANGLE,
                EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE,
                EGL_PLATFORM_ANGLE_ENABLE_AUTOMATIC_TRIM_ANGLE,
                EGL_TRUE,
                EGL_EXPERIMENTAL_PRESENT_PATH_ANGLE,
                EGL_EXPERIMENTAL_PRESENT_PATH_FAST_ANGLE,
                EGL_NONE,
            ];

            debug!("[ANGLE DEBUG] Getting EGL display and creating device...");
            let display = egl_get_platform_display_ext(
                EGL_PLATFORM_ANGLE_ANGLE,
                EGL_DEFAULT_DISPLAY,
                display_attributes.as_ptr(),
            );

            if display == EGL_NO_DISPLAY {
                log_egl_error("eglGetPlatformDisplayEXT", line!(), egl_get_error);
                return Err("Failed to get EGL display.".to_string());
            }

            if !egl_initialize(display, ptr::null_mut(), ptr::null_mut()) {
                log_egl_error("eglInitialize", line!(), egl_get_error);
                return Err("Failed to initialize EGL.".to_string());
            }

            let egl_query_display_attrib_ext: EglQueryDisplayAttribEXT =
                mem::transmute(get_proc_assert("eglQueryDisplayAttribEXT"));
            let egl_query_device_attrib_ext: EglQueryDeviceAttribEXT =
                mem::transmute(get_proc_assert("eglQueryDeviceAttribEXT"));

            let mut egl_device: isize = 0;
            if !egl_query_display_attrib_ext(display, EGL_DEVICE_EXT, &mut egl_device) {
                log_egl_error("eglQueryDisplayAttribEXT", line!(), egl_get_error);
                return Err("Failed to query EGL display attribute for device.".to_string());
            }

            let mut d3d11_device_ptr: isize = 0;
            if !egl_query_device_attrib_ext(
                egl_device as *mut c_void,
                EGL_D3D11_DEVICE_ANGLE,
                &mut d3d11_device_ptr,
            ) {
                log_egl_error("eglQueryDeviceAttribEXT", line!(), egl_get_error);
                return Err("Failed to query EGL device attribute for D3D11 device.".to_string());
            }

            if d3d11_device_ptr == 0 {
                return Err("ANGLE created a null D3D11 device.".to_string());
            }

            let angle_d3d11_device: ID3D11Device = Interface::from_raw(d3d11_device_ptr as *mut _);
            let egl_choose_config: EglChooseConfig =
                mem::transmute(get_proc_assert("eglChooseConfig"));

            let egl_create_context: EglCreateContext =
                mem::transmute(get_proc_assert("eglCreateContext"));
            let egl_make_current: EglMakeCurrent =
                mem::transmute(get_proc_assert("eglMakeCurrent"));
            let egl_destroy_context: EglDestroyContext =
                mem::transmute(get_proc_assert("eglDestroyContext"));
            let egl_terminate: EglTerminate = mem::transmute(get_proc_assert("eglTerminate"));
            let gl_finish: GlFinish = mem::transmute(get_proc_assert("glFinish"));
            let egl_create_pbuffer_from_client_buffer: EglCreatePbufferFromClientBuffer =
                mem::transmute(get_proc_assert("eglCreatePbufferFromClientBuffer"));
            let egl_destroy_surface: EglDestroySurface =
                mem::transmute(get_proc_assert("eglDestroySurface"));

            let config_attribs = [
                EGL_RED_SIZE,
                8,
                EGL_GREEN_SIZE,
                8,
                EGL_BLUE_SIZE,
                8,
                EGL_ALPHA_SIZE,
                8,
                EGL_DEPTH_SIZE,
                8,
                EGL_STENCIL_SIZE,
                8,
                EGL_SURFACE_TYPE,
                EGL_PBUFFER_BIT,
                EGL_RENDERABLE_TYPE,
                EGL_OPENGL_ES2_BIT,
                EGL_NONE,
            ];
            let mut config: *mut c_void = ptr::null_mut();
            let mut num_config = 0;

            if !egl_choose_config(
                display,
                config_attribs.as_ptr(),
                &mut config,
                1,
                &mut num_config,
            ) || num_config == 0
            {
                return Err("eglChooseConfig failed.".to_string());
            }

            info!("[AngleInterop] ANGLE initialized successfully with provided device.");
            Ok(Box::new(Self {
                egl_make_current,
                egl_get_error,
                egl_destroy_context,
                egl_terminate,
                egl_create_context,
                gl_finish,
                egl_create_pbuffer_from_client_buffer,
                egl_destroy_surface,
                display,
                context: EGL_NO_CONTEXT,
                resource_context: EGL_NO_CONTEXT,
                angle_d3d11_device,
                config,
                pbuffer_surface: EGL_NO_SURFACE,
                main_thread_id: None,
                resource_thread_id: None,
            }))
        }
    }

    ///
    /// Returns a cloned handle to the D3D11 device that was created and is managed by ANGLE.
    ///
    pub fn get_d3d_device(&self) -> Result<ID3D11Device, String> {
        Ok(self.angle_d3d11_device.clone())
    }

    ///
    /// Destroys the current EGL pbuffer surface and detaches the EGL context from the current thread.
    /// This is typically called before recreating resources for a new size.
    ///
    pub fn cleanup_surface_resources(&mut self) {
        unsafe {
            if self.pbuffer_surface != EGL_NO_SURFACE {
                info!("[AngleInterop] Cleaning up EGLSurface.");

                (self.egl_make_current)(
                    self.display,
                    EGL_NO_SURFACE,
                    EGL_NO_SURFACE,
                    EGL_NO_CONTEXT,
                );
                (self.egl_destroy_surface)(self.display, self.pbuffer_surface);
                self.pbuffer_surface = EGL_NO_SURFACE;
            }
        }
    }

    ///
    /// Recreates the underlying shared D3D11 texture and the associated EGL pbuffer surface.
    /// This is necessary when the overlay is resized.
    ///
    /// # Arguments
    ///
    /// * `width`: The new width of the texture and surface.
    /// * `height`: The new height of the texture and surface.
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple of the new `ID3D11Texture2D` and its `HANDLE` for
    /// cross-device sharing, or an error string on failure.
    ///
    pub fn recreate_resources(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<(ID3D11Texture2D, HANDLE), String> {
        self.cleanup_surface_resources();
        let angle_device = self.get_d3d_device()?;

        let (d3d_texture, handle) =
            create_shared_texture_and_get_handle(&angle_device, width, height)
                .map_err(|e| e.to_string())?;

        unsafe {
            let pbuffer_attribs = [EGL_WIDTH, width as i32, EGL_HEIGHT, height as i32, EGL_NONE];

            let d3d_texture_ptr = d3d_texture.as_raw();
            self.pbuffer_surface = (self.egl_create_pbuffer_from_client_buffer)(
                self.display,
                EGL_D3D_TEXTURE_ANGLE as u32,
                d3d_texture_ptr as *mut c_void,
                self.config,
                pbuffer_attribs.as_ptr(),
            );

            if self.pbuffer_surface == EGL_NO_SURFACE {
                log_egl_error(
                    "eglCreatePbufferFromClientBuffer",
                    line!(),
                    self.egl_get_error,
                );
                return Err("Failed to create pbuffer surface.".to_string());
            }

            info!("[AngleInterop] New EGLSurface created successfully for texture.");
        }
        Ok((d3d_texture, handle))
    }
}

///
/// Handles the complete teardown of the EGL context, display, and associated resources
/// when the `AngleInteropState` instance is dropped.
///
impl Drop for AngleInteropState {
    fn drop(&mut self) {
        unsafe {
            info!(
                "[AngleInterop] Dropping AngleInteropState on thread {:?}.",
                std::thread::current().id()
            );
            self.cleanup_surface_resources();
            (self.egl_make_current)(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
            if self.context != EGL_NO_CONTEXT {
                (self.egl_destroy_context)(self.display, self.context);
            }
            if self.resource_context != EGL_NO_CONTEXT {
                (self.egl_destroy_context)(self.display, self.resource_context);
            }
            if self.display != EGL_NO_DISPLAY {
                (self.egl_terminate)(self.display);
            }
        }
    }
}

///
/// A newtype wrapper around `Box<AngleInteropState>` to mark it as `Send` and `Sync`.
///
/// # Safety
///
/// This implementation is marked `unsafe` because the underlying EGL/OpenGL contexts
/// are not inherently thread-safe. The caller must guarantee that methods on `AngleInteropState`
/// are only called on the correct thread (e.g., the main render thread or resource loading thread
/// as established during context creation).
///
#[derive(Debug)]
pub struct SendableAngleState(pub Box<AngleInteropState>);
unsafe impl Send for SendableAngleState {}
unsafe impl Sync for SendableAngleState {}

///
/// FFI callback for the Flutter engine to make the main EGL rendering context current.
///
/// This function is called by Flutter on its rendering thread. It also handles the
/// lazy initialization of the main EGL context on the first call.
///
/// # Arguments
///
/// * `user_data`: A raw pointer to the `FlutterOverlay` instance associated with this engine.
///
extern "C" fn make_current_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &mut *(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &mut overlay.angle_state {
            let state = &mut angle_state.0;

            if state.context == EGL_NO_CONTEXT {
                info!(
                    "[AngleInterop] First call on main render thread {:?}. Initializing main EGL context.",
                    std::thread::current().id()
                );
                let context_attribs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
                state.context = (state.egl_create_context)(
                    state.display,
                    state.config,
                    state.resource_context,
                    context_attribs.as_ptr(),
                );
                if state.context == EGL_NO_CONTEXT {
                    error!("[AngleInterop] Failed to create main context.");
                    return false;
                }
                state.main_thread_id = Some(std::thread::current().id());
            }

            if state.main_thread_id != Some(current().id()) {
                error!("FATAL: make_current_callback on wrong thread!");
                return false;
            }

            let result: EGLBoolean = (state.egl_make_current)(
                state.display,
                state.pbuffer_surface,
                state.pbuffer_surface,
                state.context,
            );

            if result != EGL_TRUE {
                log_egl_error("make_current_callback", line!(), state.egl_get_error);
            }
            return result == EGL_TRUE;
        }
        false
    }
}

///
/// FFI callback for the Flutter engine to make the resource-loading EGL context current.
///
/// This function is called by Flutter on its resource loading thread. It handles the
/// lazy initialization of the shared resource EGL context on the first call.
///
/// # Arguments
///
/// * `user_data`: A raw pointer to the `FlutterOverlay` instance associated with this engine.
///
extern "C" fn make_resource_current_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &mut *(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &mut overlay.angle_state {
            let state = &mut angle_state.0;
            if state.resource_context == EGL_NO_CONTEXT {
                info!(
                    "[AngleInterop] First call on resource thread {:?}. Initializing resource EGL context.",
                    std::thread::current().id()
                );
                let context_attribs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];

                state.resource_context = (state.egl_create_context)(
                    state.display,
                    state.config,
                    EGL_NO_CONTEXT,
                    context_attribs.as_ptr(),
                );

                if state.resource_context == EGL_NO_CONTEXT {
                    error!("[AngleInterop] Failed to create resource context.");
                    return false;
                }
                state.resource_thread_id = Some(std::thread::current().id());
            }

            if state.resource_thread_id != Some(current().id()) {
                error!("FATAL: make_resource_current_callback on wrong thread!");
                return false;
            }

            let result: EGLBoolean = (state.egl_make_current)(
                state.display,
                EGL_NO_SURFACE,
                EGL_NO_SURFACE,
                state.resource_context,
            );
            if result != EGL_TRUE {
                log_egl_error(
                    "make_resource_current_callback",
                    line!(),
                    state.egl_get_error,
                );
            }
            return result == EGL_TRUE;
        }
        false
    }
}

///
/// FFI callback for the Flutter engine to clear the current EGL context.
///
/// # Arguments
///
/// * `user_data`: A raw pointer to the `FlutterOverlay` instance associated with this engine.
///
extern "C" fn clear_current_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &mut *(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &mut overlay.angle_state {
            let state = &mut angle_state.0;
            (state.egl_make_current)(
                state.display,
                EGL_NO_SURFACE,
                EGL_NO_SURFACE,
                EGL_NO_CONTEXT,
            ) == EGL_TRUE
        } else {
            false
        }
    }
}

///
/// FFI callback for the Flutter engine to signal that a frame should be presented.
/// For offscreen rendering, this typically just ensures all GL commands are flushed.
///
/// # Arguments
///
/// * `user_data`: A raw pointer to the `FlutterOverlay` instance associated with this engine.
///
extern "C" fn present_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &*(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &overlay.angle_state {
            let state = &angle_state.0;

            (state.gl_finish)();
            return true;
        }
        false
    }
}

///
/// FFI callback for the Flutter engine to get the framebuffer object ID.
/// Returns 0 to indicate that Flutter should render to the default framebuffer of the current surface.
///
extern "C" fn fbo_callback(_user_data: *mut c_void) -> u32 {
    0
}

///
/// FFI callback for the Flutter engine to resolve GL/EGL function pointers.
///
/// This function is the central resolver for the engine. It queries the globally shared
/// `eglGetProcAddress` function, which was loaded when the first `AngleInteropState`
/// was initialized. The `user_data` parameter is not used by the Flutter engine for this callback.
///
extern "C" fn gl_proc_resolver_callback(_user_data: *mut c_void, proc: *const i8) -> *mut c_void {
    if let Some(shared_egl) = SHARED_EGL.get() {
        unsafe { (shared_egl.egl_get_proc_address)(proc) }
    } else {
        error!("[gl_proc_resolver] SHARED_EGL was not initialized before use!");
        ptr::null_mut()
    }
}

///
/// Retrieves the singleton `SharedEglState`, initializing it if necessary.
///
/// On the first call within the process, this function uses the provided `engine_dir`
/// to load `libEGL.dll` and `libGLESv2.dll` and caches the library handles and the
/// `eglGetProcAddress` function pointer. On all subsequent calls, it returns the
/// already-initialized state and ignores the `engine_dir` parameter.
///
/// # Arguments
///
/// * `engine_dir`: An optional path to the directory containing ANGLE libraries. Only used on the first call.
///
/// # Returns
///
/// A `Result` containing a static reference to the `SharedEglState` on success,
/// or an error string on failure.
///
fn get_or_init_shared_egl(engine_dir: Option<&Path>) -> Result<&'static SharedEglState, String> {
    SHARED_EGL.get_or_try_init(|| {
        let libegl_path = engine_dir
            .map(|d| d.join("libEGL.dll"))
            .unwrap_or_else(|| PathBuf::from("libEGL.dll"));
        let libgles_path = engine_dir
            .map(|d| d.join("libGLESv2.dll"))
            .unwrap_or_else(|| PathBuf::from("libGLESv2.dll"));

        info!(
            "[SharedEGL] Initializing for the first time with paths: {:?}, {:?}",
            libegl_path, libgles_path
        );

        let libegl = unsafe {
            Library::new(&libegl_path)
                .map_err(|e| format!("Failed to load libEGL.dll from {:?}: {}", libegl_path, e))
        }?;
        let libgles = unsafe {
            Library::new(&libgles_path).map_err(|e| {
                format!(
                    "Failed to load libGLESv2.dll from {:?}: {}",
                    libgles_path, e
                )
            })
        }?;

        let egl_get_proc_address_symbol: Symbol<EglGetProcAddress> =
            unsafe { libegl.get(b"eglGetProcAddress") }.map_err(|e| e.to_string())?;

        let egl_get_proc_address = *egl_get_proc_address_symbol;

        Ok(SharedEglState {
            libegl,
            _libgles: libgles,
            egl_get_proc_address,
        })
    })
}

///
/// Constructs the `FlutterRendererConfig` struct required by the Flutter Engine
/// for an OpenGL-based renderer.
///
/// This function populates the configuration struct with the necessary C-ABI compatible
/// callback functions that bridge the engine's rendering lifecycle events with the
/// custom ANGLE implementation.
///
pub fn build_opengl_renderer_config() -> embedder::FlutterRendererConfig {
    embedder::FlutterRendererConfig {
        type_: embedder::FlutterRendererType_kOpenGL,
        __bindgen_anon_1: embedder::FlutterRendererConfig__bindgen_ty_1 {
            open_gl: embedder::FlutterOpenGLRendererConfig {
                struct_size: std::mem::size_of::<embedder::FlutterOpenGLRendererConfig>(),
                make_current: Some(make_current_callback),
                clear_current: Some(clear_current_callback),
                present: Some(present_callback),
                fbo_callback: Some(fbo_callback),
                make_resource_current: Some(make_resource_current_callback),
                fbo_reset_after_present: false,
                gl_proc_resolver: Some(gl_proc_resolver_callback),
                surface_transformation: None,
                gl_external_texture_frame_callback: None,
                fbo_with_frame_info_callback: None,
                present_with_info: None,
                populate_existing_damage: None,
            },
        },
    }
}
