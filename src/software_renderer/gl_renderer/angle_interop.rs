use crate::bindings::embedder;

use crate::software_renderer::overlay::d3d::create_shared_texture_and_get_handle;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;

use libloading::{Library, Symbol};
use log::{debug, error, info, warn};
use std::ffi::{CString, c_void};
use std::{mem, ptr};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
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
pub const EGL_PLATFORM_ANGLE_DEVICE_TYPE_ANGLE: i32 = 0x3209;
pub const EGL_PLATFORM_ANGLE_DEVICE_TYPE_D3D_DEBUG_ANGLE: i32 = 0x3451;
pub const EGL_DXGI_KEYED_MUTEX_ANGLE: i32 = 0x33A2;

pub const EGL_PLATFORM_ANGLE_ANGLE: i32 = 0x3202;
pub const EGL_PLATFORM_ANGLE_TYPE_ANGLE: i32 = 0x3203;
pub const EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE: i32 = 0x3208;
pub const EGL_PLATFORM_ANGLE_TYPE_D3D11_WARP_ANGLE: i32 = 0x320B;
pub const EGL_PLATFORM_ANGLE_ENABLE_AUTOMATIC_TRIM_ANGLE: i32 = 0x320F;
pub const EGL_PLATFORM_ANGLE_MAX_VERSION_MAJOR_ANGLE: i32 = 0x3204;
pub const EGL_PLATFORM_ANGLE_MAX_VERSION_MINOR_ANGLE: i32 = 0x3205;
pub const EGL_PLATFORM_ANGLE_D3D11_ANGLE: i32 = 0x3208;
pub const EGL_EXPERIMENTAL_PRESENT_PATH_ANGLE: i32 = 0x33A4;
pub const EGL_EXPERIMENTAL_PRESENT_PATH_FAST_ANGLE: i32 = 0x33A9;
pub const EGL_PLATFORM_ANGLE_D3D_TEXTURE_FORMAT_ANGLE: i32 = 0x34A7;

pub const EGL_DEVICE_EXT: i32 = 0x322C;
pub const EGL_D3D11_DEVICE_ANGLE: i32 = 0x33A1;
pub const EGL_D3D_TEXTURE_ANGLE: i32 = 0x33A3;
pub const EGL_GL_COLORSPACE_KHR: i32 = 0x309D;
pub const EGL_GL_COLORSPACE_SRGB_KHR: i32 = 0x3089;
pub const EGL_PLATFORM_ANGLE_D3D_DEVICE_ANGLE: i32 = 0x33A1;
pub const EGL_TEXTURE_INTERNAL_FORMAT_ANGLE: i32 = 0x345D;

pub const GL_FRAMEBUFFER: u32 = 0x8D40;
pub const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
pub const GL_TEXTURE_2D_GL: u32 = 0x0DE1;
pub const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;

type EglGetProcAddress = unsafe extern "C" fn(*const i8) -> *mut c_void;

type EglGetPlatformDisplayEXT = unsafe extern "C" fn(i32, *mut c_void, *const i32) -> *mut c_void;
type EglGetPlatformDisplay = unsafe extern "C" fn(i32, *mut c_void, *const i32) -> *mut c_void;
type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> bool;
type EglChooseConfig =
    unsafe extern "C" fn(*mut c_void, *const i32, *mut *mut c_void, i32, *mut i32) -> bool;
type EglCreateContext =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglMakeCurrent =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> bool;
type EglDestroyContext = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
type EglTerminate = unsafe extern "C" fn(*mut c_void) -> bool;
type EglGetError = unsafe extern "C" fn() -> i32;
type EglQueryDisplayAttribEXT = unsafe extern "C" fn(*mut c_void, i32, *mut isize) -> bool;
type EglQueryDeviceAttribEXT = unsafe extern "C" fn(*mut c_void, i32, *mut isize) -> bool;
type EglCreateImageKHR =
    unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void, *const isize) -> *mut c_void;
type EglDestroyImageKHR = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;

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
type GlEGLImageTargetTexture2DOES = unsafe extern "C" fn(u32, *mut c_void);

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
    pub egl_create_image_khr: EglCreateImageKHR,
    gl_gen_framebuffers: GlGenFramebuffers,
    gl_bind_framebuffer: GlBindFramebuffer,
    gl_framebuffer_texture_2d: GlFramebufferTexture2D,
    gl_check_framebuffer_status: GlCheckFramebufferStatus,
    gl_gen_textures: GlGenTextures,
    egl_destroy_image_khr: EglDestroyImageKHR,
    pub egl_image: *mut c_void,
    gl_bind_texture: GlBindTexture,
    pub gl_delete_textures: GlDeleteTextures,
    pub gl_delete_framebuffers: GlDeleteFramebuffers,
    gl_flush: GlFlush,
    gl_finish: GlFinish,
    pub gl_egl_image_target_texture_2d_oes: GlEGLImageTargetTexture2DOES,
    pub display: *mut c_void,
    pub context: *mut c_void,
    pub resource_context: *mut c_void,
    pub fbo_id: u32,
    pub gl_texture_id: u32,
    egl_destroy_context: EglDestroyContext,
    egl_terminate: EglTerminate,
    pub angle_d3d11_device: ID3D11Device,
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

            let egl_get_platform_display_ext: EglGetPlatformDisplayEXT =
                mem::transmute(get_proc("eglGetPlatformDisplayEXT"));
            if egl_get_platform_display_ext as *const c_void == ptr::null() {
                return Err("eglGetPlatformDisplayEXT not available".to_string());
            }
            let egl_initialize: EglInitialize = mem::transmute(get_proc("eglInitialize"));
            let egl_get_error: EglGetError = mem::transmute(get_proc("eglGetError"));

            // Use Flutter's display attributes to ask ANGLE to create a D3D11 device.
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

            // Now that EGL is initialized, we can load all other functions.
            let get_proc_assert = |name: &str| {
                let ptr = get_proc(name);
                assert!(!ptr.is_null(), "Failed to load {}", name);
                ptr
            };

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

            // Safely create an ID3D11Device object from the raw pointer.
            // The `from_raw` call correctly handles the reference count.
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
            let egl_create_image_khr: EglCreateImageKHR =
                mem::transmute(get_proc_assert("eglCreateImageKHR"));
            let gl_gen_framebuffers: GlGenFramebuffers =
                mem::transmute(get_proc_assert("glGenFramebuffers"));
            let gl_bind_framebuffer: GlBindFramebuffer =
                mem::transmute(get_proc_assert("glBindFramebuffer"));
            let gl_framebuffer_texture_2d: GlFramebufferTexture2D =
                mem::transmute(get_proc_assert("glFramebufferTexture2D"));
            let gl_check_framebuffer_status: GlCheckFramebufferStatus =
                mem::transmute(get_proc_assert("glCheckFramebufferStatus"));
            let gl_gen_textures: GlGenTextures = mem::transmute(get_proc_assert("glGenTextures"));
            let gl_bind_texture: GlBindTexture = mem::transmute(get_proc_assert("glBindTexture"));
            let gl_delete_textures: GlDeleteTextures =
                mem::transmute(get_proc_assert("glDeleteTextures"));
            let gl_delete_framebuffers: GlDeleteFramebuffers =
                mem::transmute(get_proc_assert("glDeleteFramebuffers"));
            let gl_flush: GlFlush = mem::transmute(get_proc_assert("glFlush"));
            let gl_finish: GlFinish = mem::transmute(get_proc_assert("glFinish"));
            let gl_egl_image_target_texture_2d_oes: GlEGLImageTargetTexture2DOES =
                mem::transmute(get_proc_assert("glEGLImageTargetTexture2DOES"));
            let egl_destroy_image_khr: EglDestroyImageKHR =
                mem::transmute(get_proc_assert("eglDestroyImageKHR"));

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

            let context_attribs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
            let context =
                egl_create_context(display, config, EGL_NO_CONTEXT, context_attribs.as_ptr());
            if context == EGL_NO_CONTEXT {
                return Err("eglCreateContext (main) failed.".to_string());
            }

            let resource_context =
                egl_create_context(display, config, context, context_attribs.as_ptr());
            if resource_context == EGL_NO_CONTEXT {
                egl_destroy_context(display, context);
                return Err("eglCreateContext (resource) failed.".to_string());
            }

            info!("[AngleInterop] ANGLE initialized successfully with provided device.");
            Ok(Box::new(Self {
                libegl,
                _libgles: libgles,
                egl_make_current,
                egl_get_error,
                egl_create_image_khr,
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
                gl_egl_image_target_texture_2d_oes,
                display,
                context,
                resource_context,
                fbo_id: 0,
                gl_texture_id: 0,
                egl_destroy_context,
                egl_terminate,
                egl_image: ptr::null_mut(),
                angle_d3d11_device,
                egl_destroy_image_khr,
            }))
        }
    }

    pub fn get_d3d_device(&self) -> Result<ID3D11Device, String> {
        Ok(self.angle_d3d11_device.clone())
    }

    pub fn cleanup_surface_resources(&mut self) {
        unsafe {
            (self.egl_make_current)(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, self.context);

            (self.gl_bind_framebuffer)(GL_FRAMEBUFFER, 0);

            if self.fbo_id != 0 {
                (self.gl_delete_framebuffers)(1, &self.fbo_id);
                self.fbo_id = 0;
            }
            if self.gl_texture_id != 0 {
                (self.gl_delete_textures)(1, &self.gl_texture_id);
                self.gl_texture_id = 0;
            }

            if !self.egl_image.is_null() {
                (self.egl_destroy_image_khr)(self.display, self.egl_image);
                self.egl_image = ptr::null_mut();
            }

            (self.egl_make_current)(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
        }
    }

    pub fn recreate_resources(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<(ID3D11Texture2D, HANDLE), String> {
        self.cleanup_surface_resources();
        let angle_device = self.get_d3d_device()?;

        create_shared_texture_and_get_handle(&angle_device, width, height)
            .map_err(|e| e.to_string())
    }
}
impl Drop for AngleInteropState {
    fn drop(&mut self) {
        unsafe {
            (self.egl_make_current)(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
            if self.fbo_id != 0 {
                (self.gl_delete_framebuffers)(1, &self.fbo_id);
            }
            if self.gl_texture_id != 0 {
                (self.gl_delete_textures)(1, &self.gl_texture_id);
            }
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
        let overlay = &*(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &overlay.angle_state {
            let state = &angle_state.0;
            return (state.egl_make_current)(
                state.display,
                EGL_NO_SURFACE,
                EGL_NO_SURFACE,
                state.context,
            ) == (EGL_TRUE == 1);
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
            ) == (EGL_TRUE == 1);
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
            ) == (EGL_TRUE == 1);
        }
        false
    }
}
extern "C" fn present_callback(user_data: *mut c_void) -> bool {
    unsafe {
        let overlay = &*(user_data as *mut FlutterOverlay);
        if let Some(angle_state) = &overlay.angle_state {
            let state = &angle_state.0;

            (state.gl_finish)();

            (state.gl_bind_framebuffer)(GL_FRAMEBUFFER, 0);

            if let Some(mutex) = &overlay.keyed_mutex {
                let _ = mutex.ReleaseSync(1);
            }
            return true;
        }
        false
    }
}

extern "C" fn fbo_callback(user_data: *mut c_void) -> u32 {
    unsafe {
        let overlay = &mut *(user_data as *mut FlutterOverlay);

        if let Some(mutex) = &overlay.keyed_mutex {
            if let Err(e) = mutex.AcquireSync(0, u32::MAX) {
                error!(
                    "[AngleInterop] Failed to acquire keyed mutex in fbo_callback: {}",
                    e
                );
                return 0;
            }
        }

        if let Some(angle_state) = &mut overlay.angle_state {
            let state = &mut angle_state.0;

            info!("[AngleInterop] Attempting to make EGL resource context current.");
            if !make_resource_current_callback(user_data) {
                error!("[AngleInterop] Failed to make EGL resource context current.");
                if let Some(mutex) = &overlay.keyed_mutex {
                    let _ = mutex.ReleaseSync(0);
                }
                return 0;
            }
            info!("[AngleInterop] EGL resource context is current.");

            if state.fbo_id != 0 {
                info!(
                    "[AngleInterop] Returning existing FBO (ID: {}).",
                    state.fbo_id
                );
                return state.fbo_id;
            }

            if let Some(d3d_texture) = &overlay.gl_internal_linear_texture {
                let d3d_texture_ptr = d3d_texture.as_raw();

                if d3d_texture_ptr.is_null() {
                    error!("[AngleInterop] d3d_texture_ptr is null!");
                    if let Some(mutex) = &overlay.keyed_mutex {
                        let _ = mutex.ReleaseSync(0);
                    }
                    return 0;
                }
                info!("[AngleInterop] d3d_texture_ptr is valid.");

                let mut texture_desc: D3D11_TEXTURE2D_DESC = std::mem::zeroed();
                d3d_texture.GetDesc(&mut texture_desc);

                // Add these checks and log the results
                info!("[AngleInterop] Texture Description:");
                info!("- Width: {}", texture_desc.Width);
                info!("- Height: {}", texture_desc.Height);
                info!("- Format: {:#X}", texture_desc.Format.0);
                info!("- Usage: {:#X}", texture_desc.Usage.0);
                info!("- MiscFlags: {:#X}", texture_desc.MiscFlags);
                info!("- BindFlags: {:#X}", texture_desc.BindFlags);
                info!("- CPUAccessFlags: {:#X}", texture_desc.CPUAccessFlags);

                // Create a simple attribute list.
                // According to the ANGLE extension docs, EGL_D3D_TEXTURE_ANGLE
                // does not accept EGL_DXGI_KEYED_MUTEX_ANGLE as an attribute.
                let image_attribs = [EGL_NONE as isize];

                info!("-----------------------------------------------------------------");
                info!("[AngleInterop] DEBUGGING eglCreateImageKHR CALL (Corrected)");
                info!("- Display: {:p}", state.display);
                info!("- Context: {:p} (should be EGL_NO_CONTEXT)", EGL_NO_CONTEXT);
                info!(
                    "- Target: {:#X} (EGL_D3D_TEXTURE_ANGLE)",
                    EGL_D3D_TEXTURE_ANGLE
                );
                info!("- Buffer: {:p}", d3d_texture_ptr);
                info!(
                    "- Attribs (count: {}): {:?}",
                    image_attribs.len(),
                    image_attribs
                );
                info!("-----------------------------------------------------------------");

                let egl_image = (state.egl_create_image_khr)(
                    state.display,
                    EGL_NO_CONTEXT,
                    EGL_D3D_TEXTURE_ANGLE as u32,
                    d3d_texture_ptr as *mut c_void,
                    image_attribs.as_ptr() as *const isize,
                );

                if egl_image.is_null() {
                    error!("[AngleInterop] eglCreateImageKHR failed.");
                    log_egl_error("fbo_callback", line!(), state.egl_get_error);
                    if let Some(mutex) = &overlay.keyed_mutex {
                        let _ = mutex.ReleaseSync(0);
                    }
                    return 0;
                }

                info!(
                    "[AngleInterop] eglCreateImageKHR succeeded! EGL Image: {:p}",
                    egl_image
                );

                state.egl_image = egl_image;
                (state.gl_gen_textures)(1, &mut state.gl_texture_id);
                (state.gl_bind_texture)(GL_TEXTURE_2D_GL, state.gl_texture_id);
                (state.gl_egl_image_target_texture_2d_oes)(GL_TEXTURE_2D_GL, egl_image);
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
                        "[AngleInterop] Framebuffer is not complete. Status: {:#X}",
                        status
                    );

                    (state.gl_bind_framebuffer)(GL_FRAMEBUFFER, 0);
                    (state.gl_delete_framebuffers)(1, &state.fbo_id);
                    (state.gl_delete_textures)(1, &state.gl_texture_id);
                    (state.egl_destroy_image_khr)(state.display, state.egl_image);
                    state.fbo_id = 0;
                    state.gl_texture_id = 0;
                    state.egl_image = ptr::null_mut();

                    if let Some(mutex) = &overlay.keyed_mutex {
                        let _ = mutex.ReleaseSync(0);
                    }
                    return 0;
                }
                info!(
                    "[AngleInterop] FBO (via EGLImage) created successfully (ID: {}).",
                    state.fbo_id
                );
                return state.fbo_id;
            }
        }
        warn!("[ANGLE DEBUG] fbo_callback executed but no angle_state or texture was present.");
        if let Some(mutex) = &overlay.keyed_mutex {
            let _ = mutex.ReleaseSync(0);
        }
        0
    }
}

extern "C" fn gl_proc_resolver_callback(_user_data: *mut c_void, proc: *const i8) -> *mut c_void {
    unsafe {
        let libegl = Library::new(r"E:\nier_dev_tools\nams-rs\target\libEGL.dll").unwrap();
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
