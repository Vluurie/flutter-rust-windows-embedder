//! OpenGL backing-store create/collect callbacks for the multi-view compositor.
//!
//! When a [`FlutterCompositor`] is installed, the engine no longer renders into
//! the surface bound by `make_current`. Instead, for every layer of every view
//! it calls [`create_backing_store_callback`] asking the embedder for a render
//! target, renders into it, and later hands the composed layers back via the
//! present-view callback.
//!
//! For the ANGLE/D3D11 path we satisfy each backing store with an **OpenGL
//! framebuffer** whose color attachment is a GL texture that aliases a shared
//! D3D11 texture (via an EGL pbuffer / `eglBindTexImage`-style binding done in
//! [`view_surface`]). Flutter renders GL into the FBO; the same pixels are
//! visible to the host's D3D11 device through the shared texture.
//!
//! The backing store's `user_data` baton points at a heap-allocated
//! [`GlBackingStore`] that owns the GL object names so the destruction callback
//! can free them on an engine thread.
//!
//! [`FlutterCompositor`]: crate::bindings::embedder::FlutterCompositor
//! [`view_surface`]: super::view_surface

use std::ffi::c_void;

use log::error;

use crate::bindings::embedder::{
    self as e, FlutterBackingStore, FlutterBackingStoreType_kFlutterBackingStoreTypeOpenGL,
    FlutterOpenGLBackingStore, FlutterOpenGLBackingStore__bindgen_ty_1, FlutterOpenGLFramebuffer,
    FlutterOpenGLTargetType_kFlutterOpenGLTargetTypeFramebuffer,
};

/// GL format for the framebuffer color attachment. ANGLE renders BGRA into the
/// shared `DXGI_FORMAT_B8G8R8A8_UNORM` texture; `GL_BGRA8_EXT` is the matching
/// sized format. We report `GL_RGBA8` to the engine via the framebuffer `target`
/// field because the engine only needs a generic sized format hint.
pub const GL_RGBA8: u32 = 0x8058;

/// Heap-owned GL resources for one backing store, pointed at by the backing
/// store's `user_data`. Owned by the embedder; freed in [`collect`].
pub struct GlBackingStore {
    /// GL framebuffer object name the engine renders into.
    pub fbo: u32,
    /// GL texture name used as the FBO color attachment (aliases the shared
    /// D3D11 texture for this view).
    pub color_texture: u32,
    /// The view this backing store belongs to. Lets the collect callback and the
    /// present path correlate the FBO with the right [`ViewSurface`].
    ///
    /// [`ViewSurface`]: super::view_surface::ViewSurface
    pub view_id: e::FlutterViewId,
    /// Backing size, used to validate the engine's requested config.
    pub width: usize,
    pub height: usize,
}

/// Populates `backing_store_out` describing a GL framebuffer for the engine to
/// render `config.view_id` into.
///
/// `gl` carries the already-created GL object names (FBO + color texture) for
/// this view; see [`view_surface`] for how those are produced from the shared
/// D3D11 texture. The returned `user_data` is the boxed [`GlBackingStore`] so
/// [`collect`] can drop it.
///
/// [`view_surface`]: super::view_surface
///
/// # Safety
/// `backing_store_out` must be a valid, writable `FlutterBackingStore` pointer
/// supplied by the engine for the duration of the create-backing-store callback.
pub unsafe fn fill_opengl_backing_store(
    backing_store_out: *mut FlutterBackingStore,
    gl: Box<GlBackingStore>,
) -> bool {
    if backing_store_out.is_null() {
        error!("[compositor] create_backing_store: null out pointer");
        return false;
    }

    let user_data = Box::into_raw(gl) as *mut c_void;

    let framebuffer = FlutterOpenGLFramebuffer {
        // `target` is (per the header @bug note) actually the color *format*.
        target: GL_RGBA8,
        name: unsafe { (*(user_data as *mut GlBackingStore)).fbo },
        user_data,
        destruction_callback: Some(collect_gl_backing_store_destruction),
    };

    let opengl = FlutterOpenGLBackingStore {
        type_: FlutterOpenGLTargetType_kFlutterOpenGLTargetTypeFramebuffer,
        __bindgen_anon_1: FlutterOpenGLBackingStore__bindgen_ty_1 { framebuffer },
    };

    unsafe {
        (*backing_store_out).struct_size = std::mem::size_of::<FlutterBackingStore>();
        (*backing_store_out).user_data = user_data;
        (*backing_store_out).type_ = FlutterBackingStoreType_kFlutterBackingStoreTypeOpenGL;
        (*backing_store_out).did_update = true;
        (*backing_store_out).__bindgen_anon_1 =
            e::FlutterBackingStore__bindgen_ty_1 { open_gl: opengl };
    }

    true
}

/// Engine-invoked destruction callback for a backing store's GL color
/// attachment. The GL objects are deleted lazily on the render thread by the
/// view surface; here we only reclaim the `GlBackingStore` box and mark the
/// names for deletion.
extern "C" fn collect_gl_backing_store_destruction(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    // Reclaim ownership and drop. The actual glDeleteFramebuffers/Textures is
    // performed by the ViewSurface when it tears down (it owns the GL context
    // and the correct thread); dropping here just frees the metadata box.
    let gl = unsafe { Box::from_raw(user_data as *mut GlBackingStore) };
    drop(gl);
}
