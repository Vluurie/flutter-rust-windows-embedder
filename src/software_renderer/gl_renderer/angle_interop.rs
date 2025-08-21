use crate::bindings::embedder;

use crate::software_renderer::overlay::d3d::create_shared_texture_and_get_handle;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;

use libloading::{Library, Symbol};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use std::ffi::{CString, c_void};
use std::{mem, ptr};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::core::Interface;

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

#[derive(Debug)]
pub struct AngleInteropState {
    libegl: Library,
    _libgles: Library,

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
    pub fn new() -> Result<Box<Self>, String> {
        unsafe {
            info!("[AngleInterop] Initializing ANGLE and letting it create a D3D11 device...");

            debug!("[ANGLE DEBUG] Loading libEGL.dll and libGLESv2.dll...");
            let libegl = Library::new(r"E:\nier_dev_tools\nams-rs\target\libEGL.dll")
                .map_err(|e| e.to_string())?;
            let libgles = Library::new(r"E:\nier_dev_tools\nams-rs\target\libGLESv2.dll")
                .map_err(|e| e.to_string())?;

            debug!("[ANGLE DEBUG] Loading eglGetProcAddress...");
            let egl_get_proc_address: Symbol<EglGetProcAddress> = libegl
                .get(b"eglGetProcAddress")
                .map_err(|e| e.to_string())?;
            let get_proc = |name: &str| -> *mut c_void {
                let c_name = CString::new(name).unwrap();
                egl_get_proc_address(c_name.as_ptr())
            };

            let get_proc_assert = |name: &str| {
                let ptr = get_proc(name);
                assert!(!ptr.is_null(), "Failed to load {}", name);
                ptr
            };

            let egl_get_platform_display_ext: EglGetPlatformDisplayEXT =
                mem::transmute(get_proc("eglGetPlatformDisplayEXT"));
            if egl_get_platform_display_ext as *const c_void == ptr::null() {
                return Err("eglGetPlatformDisplayEXT not available".to_string());
            }
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
                libegl,
                _libgles: libgles,
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

    pub fn get_d3d_device(&self) -> Result<ID3D11Device, String> {
        Ok(self.angle_d3d11_device.clone())
    }

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

#[derive(Debug)]
pub struct SendableAngleState(pub Box<AngleInteropState>);
unsafe impl Send for SendableAngleState {}
unsafe impl Sync for SendableAngleState {}

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

            if state.main_thread_id != Some(std::thread::current().id()) {
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

            if state.resource_thread_id != Some(std::thread::current().id()) {
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

extern "C" fn fbo_callback(_user_data: *mut c_void) -> u32 {
    0
}

static EGL_GET_PROC_ADDRESS: Lazy<(Library, EglGetProcAddress)> = Lazy::new(|| unsafe {
    let libegl = Library::new(r"E:\nier_dev_tools\nams-rs\target\libEGL.dll")
        .expect("Failed to load libEGL.dll for gl_proc_resolver_callback");

    let egl_get_proc_address_symbol: Symbol<EglGetProcAddress> = libegl
        .get(b"eglGetProcAddress")
        .expect("Failed to find eglGetProcAddress in libEGL.dll");

    let egl_get_proc_address_fn: EglGetProcAddress = mem::transmute(egl_get_proc_address_symbol);

    (libegl, egl_get_proc_address_fn)
});

extern "C" fn gl_proc_resolver_callback(_user_data: *mut c_void, proc: *const i8) -> *mut c_void {
    let (_lib, get_proc_fn) = &*EGL_GET_PROC_ADDRESS;

    unsafe { get_proc_fn(proc) }
}

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
