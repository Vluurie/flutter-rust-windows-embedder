use crate::bindings::embedder;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;
use libloading::{Library, Symbol};
use log::{error, info};
use std::ffi::c_void;
use std::ptr;

pub const EGL_DEFAULT_DISPLAY: *mut c_void = 0 as *mut c_void;
pub const EGL_NO_CONTEXT: *mut c_void = 0 as *mut c_void;
pub const EGL_NO_DISPLAY: *mut c_void = 0 as *mut c_void;
pub const EGL_NO_SURFACE: *mut c_void = 0 as *mut c_void;

// EGL Attribute
pub const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
pub const EGL_WIDTH: i32 = 0x3057;
pub const EGL_HEIGHT: i32 = 0x3056;
pub const EGL_TEXTURE_TARGET: i32 = 0x3081;
pub const EGL_TEXTURE_2D: i32 = 0x305F;
pub const EGL_TEXTURE_FORMAT: i32 = 0x3080;
pub const EGL_TEXTURE_RGBA: i32 = 0x3077;
pub const EGL_NONE: i32 = 0x3038;
pub const EGL_BACK_BUFFER: i32 = 0x3084;
pub const EGL_SURFACE_TYPE: i32 = 0x3033;
pub const EGL_PBUFFER_BIT: i32 = 0x0001;
pub const EGL_RENDERABLE_TYPE: i32 = 0x3040;
pub const EGL_OPENGL_ES2_BIT: i32 = 0x0004;
pub const EGL_RED_SIZE: i32 = 0x3024;
pub const EGL_GREEN_SIZE: i32 = 0x3023;
pub const EGL_BLUE_SIZE: i32 = 0x3022;
pub const EGL_ALPHA_SIZE: i32 = 0x3021;

// ANGLE-spezifische Attribute
pub const EGL_D3D_TEXTURE_2D_SHARE_HANDLE_ANGLE: i32 = 0x3200;
pub const EGL_PLATFORM_ANGLE_ANGLE: i32 = 0x3202;
pub const EGL_PLATFORM_ANGLE_TYPE_ANGLE: i32 = 0x3203;
pub const EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE: i32 = 0x3206;
pub const EGL_PLATFORM_ANGLE_DEVICE_ANGLE: i32 = 0x3204;
pub const EGL_D3D11_CREATE_DEVICE_ANGLE: i32 = 0x33A0;
pub const EGL_PLATFORM_ANGLE_MAX_VERSION_MAJOR_ANGLE: i32 = 0x320F;
pub const EGL_PLATFORM_ANGLE_MAX_VERSION_MINOR_ANGLE: i32 = 0x3210;

// EGL Fehlercodes
pub const EGL_SUCCESS: i32 = 0x3000;
pub const EGL_NOT_INITIALIZED: i32 = 0x3001;
pub const EGL_BAD_ACCESS: i32 = 0x3002;
pub const EGL_BAD_ALLOC: i32 = 0x3003;
pub const EGL_BAD_ATTRIBUTE: i32 = 0x3004;
pub const EGL_BAD_CONFIG: i32 = 0x3005;
pub const EGL_BAD_CONTEXT: i32 = 0x3006;
pub const EGL_BAD_CURRENT_SURFACE: i32 = 0x3007;
pub const EGL_BAD_DISPLAY: i32 = 0x3008;
pub const EGL_BAD_MATCH: i32 = 0x3009;
pub const EGL_BAD_NATIVE_PIXMAP: i32 = 0x300A;
pub const EGL_BAD_NATIVE_WINDOW: i32 = 0x300B;
pub const EGL_BAD_PARAMETER: i32 = 0x300C;
pub const EGL_BAD_SURFACE: i32 = 0x300D;
pub const EGL_CONTEXT_LOST: i32 = 0x300E;

// GL Konstanten
pub const GL_FRAMEBUFFER: u32 = 0x8D40;
pub const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
pub const GL_TEXTURE_2D_GL: u32 = 0x0DE1;
pub const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
pub const GL_RGBA: u32 = 0x1908;
pub const GL_UNSIGNED_BYTE: u32 = 0x1401;

type EglGetProcAddress = unsafe extern "C" fn(*const i8) -> *mut c_void;
type EglGetPlatformDisplay = unsafe extern "C" fn(i32, *mut c_void, *const isize) -> *mut c_void;
type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> bool;
type EglChooseConfig =
    unsafe extern "C" fn(*mut c_void, *const i32, *mut *mut c_void, i32, *mut i32) -> bool;
type EglCreateContext =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglCreatePbufferFromClientBuffer =
    unsafe extern "C" fn(*mut c_void, i32, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglMakeCurrent =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> bool;
pub type EglDestroySurface = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
type EglDestroyContext = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
type EglTerminate = unsafe extern "C" fn(*mut c_void) -> bool;
type EglBindTexImage = unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> bool;
type EglReleaseTexImage = unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> bool;
type EglGetError = unsafe extern "C" fn() -> i32;

type GlGenFramebuffers = unsafe extern "C" fn(i32, *mut u32);
type GlBindFramebuffer = unsafe extern "C" fn(u32, u32);
type GlFramebufferTexture2D = unsafe extern "C" fn(u32, u32, u32, u32, i32);
type GlCheckFramebufferStatus = unsafe extern "C" fn(u32) -> u32;
type GlGenTextures = unsafe extern "C" fn(i32, *mut u32);
type GlBindTexture = unsafe extern "C" fn(u32, u32);
type GlDeleteTextures = unsafe extern "C" fn(i32, *const u32);
type GlDeleteFramebuffers = unsafe extern "C" fn(i32, *const u32);
type GlFlush = unsafe extern "C" fn();
type GlFinish = unsafe extern "C" fn();

fn egl_error_to_string(error_code: i32) -> &'static str {
    match error_code {
        EGL_SUCCESS => "EGL_SUCCESS",
        EGL_NOT_INITIALIZED => "EGL_NOT_INITIALIZED",
        EGL_BAD_ACCESS => "EGL_BAD_ACCESS",
        EGL_BAD_ALLOC => "EGL_BAD_ALLOC",
        EGL_BAD_ATTRIBUTE => "EGL_BAD_ATTRIBUTE",
        EGL_BAD_CONFIG => "EGL_BAD_CONFIG",
        EGL_BAD_CONTEXT => "EGL_BAD_CONTEXT",
        EGL_BAD_CURRENT_SURFACE => "EGL_BAD_CURRENT_SURFACE",
        EGL_BAD_DISPLAY => "EGL_BAD_DISPLAY",
        EGL_BAD_MATCH => "EGL_BAD_MATCH",
        EGL_BAD_NATIVE_PIXMAP => "EGL_BAD_NATIVE_PIXMAP",
        EGL_BAD_NATIVE_WINDOW => "EGL_BAD_NATIVE_WINDOW",
        EGL_BAD_PARAMETER => "EGL_BAD_PARAMETER",
        EGL_BAD_SURFACE => "EGL_BAD_SURFACE",
        EGL_CONTEXT_LOST => "EGL_CONTEXT_LOST",
        _ => "Unknown EGL error",
    }
}

#[derive(Debug)]
pub struct AngleInteropState {
    libegl: Library,
    _libgles: Library,
    egl_get_platform_display: EglGetPlatformDisplay,
    egl_initialize: EglInitialize,
    egl_choose_config: EglChooseConfig,
    egl_create_context: EglCreateContext,
    pub egl_create_pbuffer_from_client_buffer: EglCreatePbufferFromClientBuffer,
    pub egl_make_current: EglMakeCurrent,
    pub egl_destroy_surface: EglDestroySurface,
    egl_destroy_context: EglDestroyContext,
    egl_terminate: EglTerminate,
    egl_bind_tex_image: EglBindTexImage,
    egl_release_tex_image: EglReleaseTexImage,
    egl_get_error: EglGetError,

    gl_gen_framebuffers: GlGenFramebuffers,
    gl_bind_framebuffer: GlBindFramebuffer,
    gl_framebuffer_texture_2d: GlFramebufferTexture2D,
    gl_check_framebuffer_status: GlCheckFramebufferStatus,
    gl_gen_textures: GlGenTextures,
    gl_bind_texture: GlBindTexture,
    pub gl_delete_textures: GlDeleteTextures,
    pub gl_delete_framebuffers: GlDeleteFramebuffers,
    gl_flush: GlFlush,
    gl_finish: GlFinish,

    pub display: *mut c_void,
    pub config: *mut c_void,
    pub context: *mut c_void,
    resource_context: *mut c_void,
    pub fbo_id: u32,
    pub gl_texture_id: u32,
    pub surface: *mut c_void,
}

impl AngleInteropState {
    pub fn new(d3d11_device: *mut c_void) -> Result<Self, String> {
        unsafe {
            info!("[AngleInterop] Initializing ANGLE/EGL for D3D11 interop...");

            let libegl = Library::new(r"E:\nier_dev_tools\nams-rs\target\libEGL.dll")
                .map_err(|e| e.to_string())?;
            let libgles = Library::new(r"E:\nier_dev_tools\nams-rs\target\libGLESv2.dll")
                .map_err(|e| e.to_string())?;

            let egl_get_proc_address: Symbol<EglGetProcAddress> = libegl
                .get(b"eglGetProcAddress")
                .map_err(|e| e.to_string())?;

            let get_proc_assert = |name: &str| -> *mut c_void {
                let c_name = std::ffi::CString::new(name).unwrap();
                let ptr = egl_get_proc_address(c_name.as_ptr());
                assert!(!ptr.is_null(), "Failed to load EGL/GL function: {}", name);
                ptr
            };

            let egl_get_platform_display: EglGetPlatformDisplay =
                std::mem::transmute(get_proc_assert("eglGetPlatformDisplay"));
            let egl_initialize: EglInitialize =
                std::mem::transmute(get_proc_assert("eglInitialize"));
            let egl_choose_config: EglChooseConfig =
                std::mem::transmute(get_proc_assert("eglChooseConfig"));
            let egl_create_context: EglCreateContext =
                std::mem::transmute(get_proc_assert("eglCreateContext"));
            let egl_create_pbuffer_from_client_buffer: EglCreatePbufferFromClientBuffer =
                std::mem::transmute(get_proc_assert("eglCreatePbufferFromClientBuffer"));
            let egl_make_current: EglMakeCurrent =
                std::mem::transmute(get_proc_assert("eglMakeCurrent"));
            let egl_destroy_surface: EglDestroySurface =
                std::mem::transmute(get_proc_assert("eglDestroySurface"));
            let egl_destroy_context: EglDestroyContext =
                std::mem::transmute(get_proc_assert("eglDestroyContext"));
            let egl_terminate: EglTerminate = std::mem::transmute(get_proc_assert("eglTerminate"));
            let egl_bind_tex_image: EglBindTexImage =
                std::mem::transmute(get_proc_assert("eglBindTexImage"));
            let egl_release_tex_image: EglReleaseTexImage =
                std::mem::transmute(get_proc_assert("eglReleaseTexImage"));
            let egl_get_error: EglGetError = std::mem::transmute(get_proc_assert("eglGetError"));
            let gl_gen_framebuffers: GlGenFramebuffers =
                std::mem::transmute(get_proc_assert("glGenFramebuffers"));
            let gl_bind_framebuffer: GlBindFramebuffer =
                std::mem::transmute(get_proc_assert("glBindFramebuffer"));
            let gl_framebuffer_texture_2d: GlFramebufferTexture2D =
                std::mem::transmute(get_proc_assert("glFramebufferTexture2D"));
            let gl_check_framebuffer_status: GlCheckFramebufferStatus =
                std::mem::transmute(get_proc_assert("glCheckFramebufferStatus"));
            let gl_gen_textures: GlGenTextures =
                std::mem::transmute(get_proc_assert("glGenTextures"));
            let gl_bind_texture: GlBindTexture =
                std::mem::transmute(get_proc_assert("glBindTexture"));
            let gl_delete_textures: GlDeleteTextures =
                std::mem::transmute(get_proc_assert("glDeleteTextures"));
            let gl_delete_framebuffers: GlDeleteFramebuffers =
                std::mem::transmute(get_proc_assert("glDeleteFramebuffers"));
            let gl_flush: GlFlush = std::mem::transmute(get_proc_assert("glFlush"));
            let gl_finish: GlFinish = std::mem::transmute(get_proc_assert("glFinish"));

            let display_attribs = [
                EGL_PLATFORM_ANGLE_TYPE_ANGLE as isize,
                EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE as isize,
                EGL_PLATFORM_ANGLE_DEVICE_ANGLE as isize,
                d3d11_device as isize,
                EGL_NONE as isize,
            ];

            let display = egl_get_platform_display(
                EGL_PLATFORM_ANGLE_ANGLE,
                EGL_DEFAULT_DISPLAY,
                display_attribs.as_ptr(),
            );
            if display == EGL_NO_DISPLAY {
                let error_code = egl_get_error();
                let error_str = egl_error_to_string(error_code);
                let msg = format!(
                    "eglGetPlatformDisplay failed with error: {} ({:#X})",
                    error_str, error_code
                );
                error!("[AngleInterop] {}", msg);
                return Err(msg);
            }

            if !egl_initialize(display, ptr::null_mut(), ptr::null_mut()) {
                let error_code = egl_get_error();
                let error_str = egl_error_to_string(error_code);
                let msg = format!(
                    "eglInitialize failed with error: {} ({:#X})",
                    error_str, error_code
                );
                error!("[AngleInterop] {}", msg);
                return Err(msg);
            }

            let mut config: *mut c_void = ptr::null_mut();
            let mut num_config = 0;
            let config_attribs = [
                EGL_RED_SIZE,
                8,
                EGL_GREEN_SIZE,
                8,
                EGL_BLUE_SIZE,
                8,
                EGL_ALPHA_SIZE,
                8,
                EGL_SURFACE_TYPE,
                EGL_PBUFFER_BIT,
                EGL_RENDERABLE_TYPE,
                EGL_OPENGL_ES2_BIT,
                EGL_NONE,
            ];
            if !egl_choose_config(
                display,
                config_attribs.as_ptr(),
                &mut config,
                1,
                &mut num_config,
            ) || num_config == 0
            {
                let error_code = egl_get_error();
                let error_str = egl_error_to_string(error_code);
                let msg = format!(
                    "eglChooseConfig failed with error: {} ({:#X})",
                    error_str, error_code
                );
                error!("[AngleInterop] {}", msg);
                return Err(msg);
            }

            let context_attribs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
            let context =
                egl_create_context(display, config, EGL_NO_CONTEXT, context_attribs.as_ptr());
            if context == EGL_NO_CONTEXT {
                let error_code = egl_get_error();
                let error_str = egl_error_to_string(error_code);
                let msg = format!(
                    "eglCreateContext for main context failed with error: {} ({:#X})",
                    error_str, error_code
                );
                error!("[AngleInterop] {}", msg);
                return Err(msg);
            }

            let resource_context =
                egl_create_context(display, config, context, context_attribs.as_ptr());
            if resource_context == EGL_NO_CONTEXT {
                let error_code = egl_get_error();
                let error_str = egl_error_to_string(error_code);
                let msg = format!(
                    "eglCreateContext for resource context failed with error: {} ({:#X})",
                    error_str, error_code
                );
                error!("[AngleInterop] {}", msg);
                (egl_destroy_context)(display, context);
                return Err(msg);
            }

            info!("[AngleInterop] ANGLE/EGL initialized successfully.");
            Ok(Self {
                libegl,
                _libgles: libgles,
                egl_get_platform_display,
                egl_initialize,
                egl_choose_config,
                egl_create_context,
                egl_create_pbuffer_from_client_buffer,
                egl_make_current,
                egl_destroy_surface,
                egl_destroy_context,
                egl_terminate,
                egl_bind_tex_image,
                egl_release_tex_image,
                egl_get_error,
                gl_gen_framebuffers,
                gl_bind_framebuffer,
                gl_framebuffer_texture_2d,
                gl_check_framebuffer_status,
                gl_gen_textures,
                gl_bind_texture,
                gl_delete_textures,
                gl_delete_framebuffers,
                gl_flush,
                gl_finish,
                display,
                config,
                context,
                resource_context,
                fbo_id: 0,
                gl_texture_id: 0,
                surface: EGL_NO_SURFACE,
            })
        }
    }
}

impl Drop for AngleInteropState {
    fn drop(&mut self) {
        unsafe {
            (self.egl_make_current)(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
            if self.surface != EGL_NO_SURFACE {
                if !(self.egl_destroy_surface)(self.display, self.surface) {
                    let ec = (self.egl_get_error)();
                    error!(
                        "[AngleInterop] drop: eglDestroySurface failed with {:#X}",
                        ec
                    );
                }
            }
            if self.context != EGL_NO_CONTEXT {
                if !(self.egl_destroy_context)(self.display, self.context) {
                    let ec = (self.egl_get_error)();
                    error!(
                        "[AngleInterop] drop: eglDestroyContext (main) failed with {:#X}",
                        ec
                    );
                }
            }
            if self.resource_context != EGL_NO_CONTEXT {
                if !(self.egl_destroy_context)(self.display, self.resource_context) {
                    let ec = (self.egl_get_error)();
                    error!(
                        "[AngleInterop] drop: eglDestroyContext (resource) failed with {:#X}",
                        ec
                    );
                }
            }
            if self.display != EGL_NO_DISPLAY {
                if !(self.egl_terminate)(self.display) {
                    let ec = (self.egl_get_error)();
                    error!("[AngleInterop] drop: eglTerminate failed with {:#X}", ec);
                }
            }
        }
        info!("[AngleInterop] State terminated and dropped.");
    }
}

#[derive(Debug)]
pub struct SendableAngleState(pub Box<AngleInteropState>);
unsafe impl Send for SendableAngleState {}
unsafe impl Sync for SendableAngleState {}

extern "C" fn make_current_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &*(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &overlay.angle_state {
            let state = &angle_state.0;
            return (state.egl_make_current)(
                state.display,
                state.surface,
                state.surface,
                state.context,
            );
        }
        false
    }
}

extern "C" fn make_resource_current_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &*(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &overlay.angle_state {
            let state = &angle_state.0;
            return (state.egl_make_current)(
                state.display,
                EGL_NO_SURFACE,
                EGL_NO_SURFACE,
                state.resource_context,
            );
        }
        false
    }
}

extern "C" fn clear_current_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &*(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &overlay.angle_state {
            let state = &angle_state.0;
            return (state.egl_make_current)(
                state.display,
                EGL_NO_SURFACE,
                EGL_NO_SURFACE,
                EGL_NO_CONTEXT,
            );
        }
        false
    }
}

extern "C" fn fbo_callback(user_data: *mut c_void) -> u32 {
    unsafe {
        let overlay = &mut *(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &mut overlay.angle_state {
            let state = &mut angle_state.0;

            if state.fbo_id != 0 {
                return state.fbo_id;
            }

            if let Some(handle) = overlay.d3d11_shared_handle {
                let pbuffer_attribs = [
                    EGL_WIDTH,
                    overlay.width as i32,
                    EGL_HEIGHT,
                    overlay.height as i32,
                    EGL_NONE,
                ];

                state.surface = (state.egl_create_pbuffer_from_client_buffer)(
                    state.display,
                    EGL_D3D_TEXTURE_2D_SHARE_HANDLE_ANGLE,
                    handle.0.0 as *mut _,
                    state.config,
                    pbuffer_attribs.as_ptr(),
                );

                if state.surface == EGL_NO_SURFACE {
                    let error_code = (state.egl_get_error)();
                    error!(
                        "[AngleInterop] fbo_callback: eglCreatePbufferFromClientBuffer failed. EGL Error: {} ({:#X})",
                        egl_error_to_string(error_code),
                        error_code
                    );
                    return 0;
                }

                (state.gl_gen_textures)(1, &mut state.gl_texture_id);
                (state.gl_bind_texture)(GL_TEXTURE_2D_GL, state.gl_texture_id);

                if !(state.egl_bind_tex_image)(state.display, state.surface, EGL_BACK_BUFFER) {
                    let error_code = (state.egl_get_error)();
                    error!(
                        "[AngleInterop] fbo_callback: eglBindTexImage failed. EGL Error: {} ({:#X})",
                        egl_error_to_string(error_code),
                        error_code
                    );
                    (state.gl_delete_textures)(1, &state.gl_texture_id);
                    state.gl_texture_id = 0;
                    return 0;
                }

                (state.gl_gen_framebuffers)(1, &mut state.fbo_id);
                (state.gl_bind_framebuffer)(GL_FRAMEBUFFER, state.fbo_id);

                (state.gl_framebuffer_texture_2d)(
                    GL_FRAMEBUFFER,
                    GL_COLOR_ATTACHMENT0,
                    GL_TEXTURE_2D_GL,
                    state.gl_texture_id,
                    0,
                );

                let status = (state.gl_check_framebuffer_status)(GL_FRAMEBUFFER);
                if status != GL_FRAMEBUFFER_COMPLETE {
                    error!(
                        "[AngleInterop] fbo_callback: Framebuffer is not complete. Status: {:#X}",
                        status
                    );
                    (state.gl_bind_framebuffer)(GL_FRAMEBUFFER, 0);
                    (state.gl_delete_framebuffers)(1, &state.fbo_id);
                    state.fbo_id = 0;
                    return 0;
                }

                info!(
                    "[AngleInterop] fbo_callback: FBO created successfully (ID: {}) for size {}x{}.",
                    state.fbo_id, overlay.width, overlay.height
                );

                return state.fbo_id;
            }
        }
        0
    }
}

extern "C" fn present_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &*(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &overlay.angle_state {
            let state = &angle_state.0;

            (state.gl_flush)();

            if !(state.egl_release_tex_image)(state.display, state.surface, EGL_BACK_BUFFER) {
                let error_code = (state.egl_get_error)();
                error!(
                    "[AngleInterop] present_callback: eglReleaseTexImage failed. EGL Error: {} ({:#X})",
                    egl_error_to_string(error_code),
                    error_code
                );
            }
            return true;
        }
        false
    }
}

extern "C" fn gl_proc_resolver_callback(_user_data: *mut c_void, proc: *const i8) -> *mut c_void {
    unsafe {
        let libegl = Library::new("libEGL.dll").unwrap();
        let egl_get_proc_address: Symbol<EglGetProcAddress> =
            libegl.get(b"eglGetProcAddress").unwrap();
        egl_get_proc_address(proc)
    }
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
                fbo_reset_after_present: true,
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
