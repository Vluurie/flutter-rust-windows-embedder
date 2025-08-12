use crate::bindings::embedder;
use crate::software_renderer::overlay::d3d::log_texture_properties;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;
use log::{error, info, warn};
use std::cell::Cell;
use std::ffi::{CStr, c_void};
use std::ptr;
use windows::Win32::Foundation::{E_FAIL, GetLastError, HANDLE, HWND};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::core::{Error, Interface};

use windows::Win32::Graphics::Gdi::{GetDC, HDC};
use windows::Win32::Graphics::OpenGL::{
    ChoosePixelFormat, HGLRC, PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW, PFD_GENERIC_ACCELERATED,
    PFD_MAIN_PLANE, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR, SetPixelFormat,
    wglCreateContext, wglDeleteContext, wglGetProcAddress, wglMakeCurrent, wglShareLists,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::core::PCSTR;
#[repr(C)]
#[derive(Clone, Copy)]
struct GPU_DEVICE {
    cb: u32,
    DeviceName: [u8; 32],
    DeviceString: [u8; 128],
    Flags: u32,
    rcVirtualScreen: windows::Win32::Foundation::RECT,
}

const WGL_ACCESS_READ_WRITE_NV: u32 = 0x0001;

type PFNWGLDXSETRESOURCESHAREHANDLENVPROC =
    unsafe extern "system" fn(*const c_void, HANDLE) -> bool;
type PFNWGLDXOPENDEVICENVPROC = unsafe extern "system" fn(*const c_void) -> HANDLE;
type PFNWGLDXREGISTEROBJECTNVPROC =
    unsafe extern "system" fn(HANDLE, *const c_void, u32, u32, u32) -> HANDLE;

type PFNGLGENTEXTURES = unsafe extern "system" fn(i32, *mut u32);
type PFNGLGENFRAMEBUFFERS = unsafe extern "system" fn(i32, *mut u32);
type PFNGLBINDFRAMEBUFFER = unsafe extern "system" fn(u32, u32);
type PFNGLFRAMEBUFFERTEXTURE2D = unsafe extern "system" fn(u32, u32, u32, u32, i32);
type PFNGLCHECKFRAMEBUFFERSTATUS = unsafe extern "system" fn(u32) -> u32;
type PFNGLFLUSH = unsafe extern "system" fn();
type PFNGLFINISH = unsafe extern "system" fn();
type HGPUNV = isize;
type PFNWGLENUMGPUSNVPROC = unsafe extern "system" fn(u32, *mut HGPUNV) -> u32;
type PFNWGLENUMGPUDEVICESNVPROC = unsafe extern "system" fn(HGPUNV, u32, *mut GPU_DEVICE) -> u32;
type PFNWGLCREATEAFFINITYDCNVPROC = unsafe extern "system" fn(*const HGPUNV) -> HDC;
type PFNWGLDELETEDCNVPROC = unsafe extern "system" fn(HDC) -> bool;
type PFNWGLDXLOCKOBJECTSNVPROC = unsafe extern "system" fn(HANDLE, i32, *const HANDLE) -> bool;
type PFNWGLDXUNLOCKOBJECTSNVPROC = unsafe extern "system" fn(HANDLE, i32, *const HANDLE) -> bool;

pub struct GLState {
    pub hdc: HDC,
    pub hglrc: HGLRC,
    pub fbo_id: u32,
    pub gl_texture_id: u32,
    pub dx_interop_device_handle: HANDLE,
    pub dx_interop_texture_handle: HANDLE,
    pub gl_gen_framebuffers: PFNGLGENFRAMEBUFFERS,
    pub gl_bind_framebuffer: PFNGLBINDFRAMEBUFFER,
    pub gl_framebuffer_texture_2d: PFNGLFRAMEBUFFERTEXTURE2D,
    pub gl_check_framebuffer_status: PFNGLCHECKFRAMEBUFFERSTATUS,
    pub gl_flush: PFNGLFLUSH,
    pub gl_finish: PFNGLFINISH,
    pub wgl_dx_lock_objects: PFNWGLDXLOCKOBJECTSNVPROC,
    pub wgl_dx_unlock_objects: PFNWGLDXUNLOCKOBJECTSNVPROC,
}

impl GLState {
    pub fn new(
        hwnd: HWND,
        d3d_device: &ID3D11Device,
        d3d_texture: &ID3D11Texture2D,
        shared_handle: HANDLE,
    ) -> windows::core::Result<Self> {
        info!("[GLState::new] Starting NV Interop initialization...");
        unsafe {
            let hdc = GetDC(Some(hwnd));
            if hdc.is_invalid() {
                return Err(Error::new(E_FAIL, "GetDC failed."));
            }

            let pfd = PIXELFORMATDESCRIPTOR {
                nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
                nVersion: 1,
                dwFlags: PFD_DRAW_TO_WINDOW
                    | PFD_SUPPORT_OPENGL
                    | PFD_DOUBLEBUFFER
                    | PFD_GENERIC_ACCELERATED,
                iPixelType: PFD_TYPE_RGBA,
                cColorBits: 32,
                cDepthBits: 24,
                cStencilBits: 8,
                ..Default::default()
            };
            let pixel_format = ChoosePixelFormat(hdc, &pfd);
            if pixel_format == 0 {
                return Err(Error::new(E_FAIL, "ChoosePixelFormat failed."));
            }
            if !SetPixelFormat(hdc, pixel_format, &pfd).is_ok() {
                return Err(Error::new(E_FAIL, "SetPixelFormat failed."));
            }

            let hglrc = wglCreateContext(hdc)?;
            if !wglMakeCurrent(hdc, hglrc).is_ok() {
                wglDeleteContext(hglrc).ok();
                return Err(Error::new(E_FAIL, "wglMakeCurrent failed."));
            }
            info!("[GL NV] Legacy GL context created and activated.");

            let opengl32 = GetModuleHandleA(PCSTR(b"opengl32.dll\0".as_ptr())).unwrap();

            let get_proc = |name: &[u8]| -> *const c_void {
                let name_pcstr = PCSTR(name.as_ptr());
                let ptr =
                    wglGetProcAddress(name_pcstr).or_else(|| GetProcAddress(opengl32, name_pcstr));

                if let Some(p) = ptr {
                    p as *const c_void
                } else {
                    let err_str =
                        format!("Cannot resolve function: {}", String::from_utf8_lossy(name));
                    error!("{}", err_str);
                    panic!("{}", err_str);
                }
            };
            let wgl_dx_lock_objects: PFNWGLDXLOCKOBJECTSNVPROC =
                std::mem::transmute(get_proc(b"wglDXLockObjectsNV\0"));
            let wgl_dx_unlock_objects: PFNWGLDXUNLOCKOBJECTSNVPROC =
                std::mem::transmute(get_proc(b"wglDXUnlockObjectsNV\0"));

            info!("[GL NV] Loading NV interop functions...");
            let wgl_dx_open_device: PFNWGLDXOPENDEVICENVPROC =
                std::mem::transmute(get_proc(b"wglDXOpenDeviceNV\0"));
            let wgl_dx_register_object: PFNWGLDXREGISTEROBJECTNVPROC =
                std::mem::transmute(get_proc(b"wglDXRegisterObjectNV\0"));
            let wgl_dx_set_resource_share_handle: PFNWGLDXSETRESOURCESHAREHANDLENVPROC =
                std::mem::transmute(get_proc(b"wglDXSetResourceShareHandleNV\0"));
            info!("[GL NV] NV functions loaded.");

            let gl_gen_textures: PFNGLGENTEXTURES =
                std::mem::transmute(get_proc(b"glGenTextures\0"));
            let gl_gen_framebuffers: PFNGLGENFRAMEBUFFERS =
                std::mem::transmute(get_proc(b"glGenFramebuffers\0"));
            let gl_bind_framebuffer: PFNGLBINDFRAMEBUFFER =
                std::mem::transmute(get_proc(b"glBindFramebuffer\0"));
            let gl_framebuffer_texture_2d: PFNGLFRAMEBUFFERTEXTURE2D =
                std::mem::transmute(get_proc(b"glFramebufferTexture2D\0"));
            let gl_check_framebuffer_status: PFNGLCHECKFRAMEBUFFERSTATUS =
                std::mem::transmute(get_proc(b"glCheckFramebufferStatus\0"));
            let gl_flush: PFNGLFLUSH = std::mem::transmute(get_proc(b"glFlush\0"));
            let gl_finish: PFNGLFINISH = std::mem::transmute(get_proc(b"glFinish\0"));

            info!("[GL NV] Calling wglDXOpenDeviceNV...");
            let dx_interop_device_handle = wgl_dx_open_device(d3d_device.as_raw() as *const c_void);
            if dx_interop_device_handle.is_invalid() {
                error!("[GL NV] wglDXOpenDeviceNV FAILED!");
                return Err(Error::new(E_FAIL, "wglDXOpenDeviceNV failed."));
            }
            info!(
                "[GL NV] wglDXOpenDeviceNV SUCCESSFUL! Handle: {:?}",
                dx_interop_device_handle
            );

            if !wgl_dx_set_resource_share_handle(
                d3d_texture.as_raw() as *const c_void,
                shared_handle,
            ) {
                error!("[GL NV] wglDXSetResourceShareHandleNV FAILED!");
                return Err(Error::new(E_FAIL, "wglDXSetResourceShareHandleNV failed."));
            }

            let mut gl_texture_id = 0;
            gl_gen_textures(1, &mut gl_texture_id);

            const GL_TEXTURE_2D: u32 = 0x0DE1;

            info!("[GL NV] Calling wglDXRegisterObjectNV...");
            let dx_interop_texture_handle = wgl_dx_register_object(
                dx_interop_device_handle,
                d3d_texture.as_raw() as *const c_void,
                gl_texture_id,
                GL_TEXTURE_2D,
                WGL_ACCESS_READ_WRITE_NV,
            );
            if dx_interop_texture_handle.is_invalid() {
                error!("[GL NV] wglDXRegisterObjectNV FAILED!");
                return Err(Error::new(E_FAIL, "wglDXRegisterObjectNV failed."));
            }
            info!(
                "[GL NV] wglDXRegisterObjectNV SUCCESSFUL! Handle: {:?}",
                dx_interop_texture_handle
            );

            wglMakeCurrent(HDC::default(), HGLRC::default());
            info!("[GL NV] Initialization complete.");

            Ok(Self {
                hdc,
                hglrc,
                fbo_id: 0,
                gl_texture_id,
                dx_interop_device_handle,
                dx_interop_texture_handle,
                gl_gen_framebuffers,
                gl_bind_framebuffer,
                gl_framebuffer_texture_2d,
                gl_check_framebuffer_status,
                gl_flush,
                gl_finish,
                wgl_dx_lock_objects,
                wgl_dx_unlock_objects,
            })
        }
    }

    pub fn new_from_existing(existing_state: &GLState, new_hdc: HDC, new_hglrc: HGLRC) -> Self {
        Self {
            hdc: new_hdc,
            hglrc: new_hglrc,
            fbo_id: 0,
            gl_texture_id: 0,
            dx_interop_device_handle: existing_state.dx_interop_device_handle,
            dx_interop_texture_handle: existing_state.dx_interop_texture_handle,
            gl_gen_framebuffers: existing_state.gl_gen_framebuffers,
            gl_bind_framebuffer: existing_state.gl_bind_framebuffer,
            gl_framebuffer_texture_2d: existing_state.gl_framebuffer_texture_2d,
            gl_check_framebuffer_status: existing_state.gl_check_framebuffer_status,
            gl_flush: existing_state.gl_flush,
            gl_finish: existing_state.gl_finish,
            wgl_dx_lock_objects: existing_state.wgl_dx_lock_objects,
            wgl_dx_unlock_objects: existing_state.wgl_dx_unlock_objects,
        }
    }

    pub fn new_resource_context(hwnd: HWND, main_hglrc: HGLRC) -> windows::core::Result<HGLRC> {
        unsafe {
            let hdc = GetDC(Some(hwnd));
            if hdc.is_invalid() {
                return Err(Error::new(E_FAIL, "GetDC failed."));
            }

            let resource_hglrc = wglCreateContext(hdc)?;
            if !wglShareLists(main_hglrc, resource_hglrc).is_ok() {
                wglDeleteContext(resource_hglrc).ok();
                return Err(Error::new(E_FAIL, "wglShareLists failed."));
            }
            Ok(resource_hglrc)
        }
    }
}

unsafe extern "C" fn make_current_callback(user_data: *mut c_void) -> bool {
    if user_data.is_null() {
        return false;
    }

    let overlay = &*(user_data as *mut FlutterOverlay);

    if let Some(gl_state) = &overlay.gl_state {
        let state = &*gl_state.0;
        return wglMakeCurrent(state.hdc, state.hglrc).is_ok();
    }
    false
}

unsafe extern "C" fn make_resource_current_callback(user_data: *mut c_void) -> bool {
    if user_data.is_null() {
        return false;
    }

    let overlay = &*(user_data as *mut FlutterOverlay);

    if let Some(gl_resource_state) = &overlay.gl_resource_state {
        let state = &*gl_resource_state.0;
        return wglMakeCurrent(state.hdc, state.hglrc).is_ok();
    }
    false
}

unsafe extern "C" fn clear_current_callback(_user_data: *mut c_void) -> bool {
    wglMakeCurrent(HDC::default(), HGLRC::default()).is_ok()
}

unsafe extern "C" fn fbo_callback(user_data: *mut c_void) -> u32 {
    if user_data.is_null() {
        return 0;
    }
    let overlay = &mut *(user_data as *mut FlutterOverlay);

    if let Some(gl_state) = &mut overlay.gl_state {
        let state = &mut *gl_state.0;

        let lock_ok = (state.wgl_dx_lock_objects)(
            state.dx_interop_device_handle,
            1,
            &state.dx_interop_texture_handle,
        );
        if !lock_ok {
            error!("[Callback FBO] wglDXLockObjectsNV failed!");
            return 0;
        }

        if state.fbo_id == 0 {
            (state.gl_gen_framebuffers)(1, &mut state.fbo_id);
            (state.gl_bind_framebuffer)(0x8D40, state.fbo_id);
            (state.gl_framebuffer_texture_2d)(0x8D40, 0x8CE0, 0x0DE1, state.gl_texture_id, 0);

            let status = (state.gl_check_framebuffer_status)(0x8D40);
            if status != 0x8CD5 {
                error!("[Callback FBO] Framebuffer incomplete: 0x{:X}", status);

                (state.wgl_dx_unlock_objects)(
                    state.dx_interop_device_handle,
                    1,
                    &state.dx_interop_texture_handle,
                );
                return 0;
            }
        }
        return state.fbo_id;
    }
    0
}

unsafe extern "C" fn present_callback(user_data: *mut c_void) -> bool {
    if user_data.is_null() {
        return false;
    }
    let overlay = &*(user_data as *mut FlutterOverlay);

    if let Some(gl_state) = &overlay.gl_state {
        let state = &*gl_state.0;

        (state.gl_flush)();

        let unlock_ok = (state.wgl_dx_unlock_objects)(
            state.dx_interop_device_handle,
            1,
            &state.dx_interop_texture_handle,
        );
        if !unlock_ok {
            error!("[Callback Present] wglDXUnlockObjectsNV failed!");
            return false;
        }

        return true;
    }
    false
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
                surface_transformation: None,
                gl_proc_resolver: Some(gl_proc_resolver_callback),
                gl_external_texture_frame_callback: None,
                fbo_with_frame_info_callback: None,
                present_with_info: None,
                populate_existing_damage: None,
            },
        },
    }
}

unsafe extern "C" fn gl_proc_resolver_callback(
    _user_data: *mut c_void,
    proc: *const i8,
) -> *mut c_void {
    let proc_name_pcstr = PCSTR(proc as *const u8);
    let proc_addr = wglGetProcAddress(proc_name_pcstr);
    if proc_addr.is_some() {
        return proc_addr.unwrap() as *mut c_void;
    }
    let opengl32 = GetModuleHandleA(PCSTR(b"opengl32.dll\0".as_ptr())).unwrap();
    GetProcAddress(opengl32, proc_name_pcstr).map_or(ptr::null_mut(), |p| p as *mut c_void)
}
