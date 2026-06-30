//! The `FlutterCompositor` callbacks that drive multi-view rendering.
//!
//! All three callbacks receive `FlutterCompositor.user_data`, which we set to a
//! raw pointer to the engine-host [`FlutterOverlay`]. From there we reach the
//! [`ViewRegistry`] to look up per-view GPU resources by `view_id`.
//!
//! * `create_backing_store_callback`: engine asks for a render target for a
//!   layer of `config.view_id`; we hand back the view's GL framebuffer.
//! * `collect_backing_store_callback`: engine releases a backing store.
//! * `present_view_callback`: engine has finished compositing a view's layers;
//!   we flush GL, record damage, and bump the view's frame counter. Satellite
//!   views have no keyed mutex (it does not work across the three device
//!   round-trips), so there is nothing to release here; the bumped frame
//!   counter is what tells the host a new frame is ready to read.
//!
//! View `0` (the implicit in-game overlay) is handled on the host overlay's own
//! fields rather than through a [`ViewSurface`]; the callbacks special-case it.
//!
//! [`FlutterOverlay`]: crate::software_renderer::api::FlutterOverlay
//! [`ViewRegistry`]: super::ViewRegistry
//! [`ViewSurface`]: super::view_surface::ViewSurface

use std::ffi::c_void;
use std::sync::atomic::Ordering;

use log::error;

use crate::bindings::embedder::{
    FlutterBackingStore, FlutterBackingStoreConfig, FlutterCompositor, FlutterPresentViewInfo,
    FlutterRect, FlutterViewFocusChangeRequest, FlutterViewFocusState_kFocused,
};
use crate::software_renderer::gl_renderer::angle_interop::{AngleInteropState, ViewGlProcs};
use crate::software_renderer::multiview::IMPLICIT_VIEW_ID;
use crate::software_renderer::multiview::resize_decision::should_realloc_texture;
use crate::software_renderer::multiview::backing_store::{
    GlBackingStore, fill_opengl_backing_store,
};
use crate::software_renderer::multiview::view_surface::ViewSurface;
use crate::software_renderer::overlay::d3d::create_shared_texture_no_mutex;
use crate::software_renderer::overlay::overlay_impl::{FlutterOverlay, SendableHandle};

/// Engine-invoked when a `FlutterView`'s focus state changes and the embedder
/// should update native focus. For satellite views we forward focus to the
/// view's top-level window so keyboard input routes correctly. View 0 (the game
/// overlay) is left to the host's own focus handling.
///
/// Wired into `FlutterProjectArgs.view_focus_change_request_callback`; the
/// `user_data` baton is the host `*mut FlutterOverlay`.
///
/// This is a C ABI callback whose signature is dictated by the engine, so the
/// raw-pointer parameters cannot be made `unsafe` in the Rust sense.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn view_focus_change_request_callback(
    request: *const FlutterViewFocusChangeRequest,
    user_data: *mut c_void,
) {
    // Declared manually to avoid depending on a specific windows-crate module
    // path for this single user32 call. `HWND` here is the raw window handle.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetFocus(hwnd: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }

    if request.is_null() || user_data.is_null() {
        return;
    }
    let request = unsafe { &*request };
    if request.view_id == IMPLICIT_VIEW_ID {
        return;
    }
    if request.state != FlutterViewFocusState_kFocused {
        return;
    }
    let host = unsafe { &*(user_data as *mut FlutterOverlay) };
    if let Some(hwnd) = host.view_registry.with_view(request.view_id, |s| s.hwnd.0) {
        unsafe {
            SetFocus(hwnd.0);
        }
    }
}

/// Builds the [`FlutterCompositor`] descriptor pointing all callbacks at
/// `host_ptr` (a `*mut FlutterOverlay`). Stored in `FlutterProjectArgs.compositor`.
pub fn build_compositor(host_ptr: *mut FlutterOverlay) -> FlutterCompositor {
    FlutterCompositor {
        struct_size: std::mem::size_of::<FlutterCompositor>(),
        user_data: host_ptr as *mut c_void,
        create_backing_store_callback: Some(create_backing_store_callback),
        collect_backing_store_callback: Some(collect_backing_store_callback),
        // Multi-view present path. Mutually exclusive with present_layers_callback.
        present_layers_callback: None,
        avoid_backing_store_cache: false,
        present_view_callback: Some(present_view_callback),
    }
}

/// Engine asks the embedder for a backing store to render a layer of
/// `config.view_id` into.
extern "C" fn create_backing_store_callback(
    config: *const FlutterBackingStoreConfig,
    backing_store_out: *mut FlutterBackingStore,
    user_data: *mut c_void,
) -> bool {
    if config.is_null() || user_data.is_null() {
        error!("[compositor] create_backing_store: null config/user_data");
        return false;
    }
    let host = unsafe { &*(user_data as *mut FlutterOverlay) };
    let config = unsafe { &*config };
    let view_id = config.view_id;

    let width = config.size.width as usize;
    let height = config.size.height as usize;

    // The implicit view (view 0) is the in-game overlay. Its GL FBO + color
    // texture are built lazily on the render thread in `make_current_callback`
    // and stored on the host as `view0_gl`. Here we just hand those names back.
    if view_id == IMPLICIT_VIEW_ID {
        match &host.view0_gl {
            Some(gl) => {
                // Implicit view (view 0) keeps its keyed mutex; it is acquired
                // for render by the existing make_current path, so no extra
                // AcquireSync is needed here.
                let bs = GlBackingStore {
                    fbo: gl.fbo,
                    color_texture: gl.color_texture,
                    view_id,
                    width,
                    height,
                };
                return unsafe {
                    fill_opengl_backing_store(backing_store_out, Box::new(bs))
                };
            }
            None => {
                error!(
                    "[compositor] create_backing_store: view0_gl not built yet \
                     (make_current must run before the first backing-store request)"
                );
                return false;
            }
        }
    }

    // The ANGLE state lives on the host overlay and provides the EGL display +
    // pbuffer creation used to build this view's GL FBO on first use. We run on
    // the render thread here, so it is safe to create GL resources.
    let angle = match &host.angle_state {
        Some(s) => &s.0,
        None => {
            error!("[compositor] create_backing_store: host has no ANGLE state");
            return false;
        }
    };

    let (cw, ch) = (width as u32, height as u32);
    let gl = host.view_registry.with_view(view_id, |surface| {
        if should_realloc_texture((cw, ch), surface.texture_size)
            && let Err(e) = realloc_satellite_gpu(angle, surface, cw, ch)
        {
            error!("[compositor] realloc on resize failed for view {view_id}: {e}");
        }
        if surface.gl.is_none()
            && let Err(e) = build_satellite_gl(angle, surface, cw, ch)
        {
            error!("[compositor] failed to build GL FBO for view {view_id}: {e}");
            return None;
        }
        // No keyed-mutex acquire here: satellite views have no keyed mutex (it
        // does not work across the three device round-trips). The engine renders
        // directly into the shared texture; cross-device ordering is handled by
        // the GL flush at present and the window thread's present-hold.
        Some(GlBackingStore {
            fbo: surface.gl_fbo(),
            color_texture: surface.gl_color_texture(),
            view_id,
            width: cw as usize,
            height: ch as usize,
        })
    });

    match gl {
        Some(Some(gl)) => unsafe {
            fill_opengl_backing_store(backing_store_out, Box::new(gl))
        },
        Some(None) => false,
        None => {
            error!("[compositor] create_backing_store: no view surface for view_id {view_id}");
            false
        }
    }
}

/// Builds a satellite view's GL pbuffer + FBO from its shared D3D11 texture.
/// Runs on the render thread with the ANGLE context current.
fn build_satellite_gl(
    angle: &AngleInteropState,
    surface: &mut ViewSurface,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let internal = surface
        .angle_internal_texture
        .as_ref()
        .ok_or_else(|| "view surface missing angle_internal_texture".to_string())?;

    let pbuffer = unsafe { angle.create_pbuffer_for_texture(internal, width, height)? };
    // NOTE: we do NOT make this pbuffer current. The compositor's
    // create-backing-store callback runs while the engine's own surface/context
    // is current; switching the current surface here would corrupt the engine's
    // render state. `eglBindTexImage` binds the pbuffer image to the GL texture
    // that `build_gl_resources` binds on the current context — it does not
    // require the pbuffer itself to be current.
    let procs = ViewGlProcs::resolve()?;
    let resources = match ViewSurface::build_gl_resources(procs, pbuffer, |p, surf| unsafe {
        angle.bind_tex_image(p, surf)
    }) {
        Ok(r) => r,
        Err(e) => {
            unsafe { angle.destroy_pbuffer(pbuffer) };
            return Err(e);
        }
    };
    surface.gl = Some(resources);
    Ok(())
}

/// Recreates a satellite view's shared ANGLE texture at a new size. Runs on the
/// engine render thread (so touching the shared ANGLE device is safe). Drops the
/// old GL resources so `build_satellite_gl` rebuilds them against the new
/// texture; the window thread reopens the new shared handle on its own device.
fn realloc_satellite_gpu(
    angle: &AngleInteropState,
    surface: &mut ViewSurface,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let angle_device = angle.get_d3d_device()?;
    let (new_internal, new_handle) = create_shared_texture_no_mutex(&angle_device, width, height)?;

    // Drop old GL (pbuffer/FBO/texture) before swapping the backing texture.
    surface.delete_gl(|pbuffer| unsafe { angle.destroy_pbuffer(pbuffer) });

    surface.angle_internal_texture = Some(new_internal);
    surface.shared_handle = Some(SendableHandle(new_handle));
    // The new shared texture physically exists at the new size, but the engine
    // has NOT rendered a frame into it yet. Record the new size so the window
    // thread learns the realloc happened, but the window thread must NOT sample
    // it until the per-view frame counter advances past the value it captured at
    // resize time (present-hold) — otherwise it would blit an uninitialised
    // texture. There is no keyed mutex (it does not work across the three device
    // round-trips), so this counter-based handshake is the only ordering guard.
    surface.texture_size = (width, height);
    // The previous shared texture object is being dropped here. For a no-mutex
    // (D3D11_RESOURCE_MISC_SHARED) texture, any handle the window thread opened
    // against the OLD original is now stale, so clear the game-device view to
    // force the window thread to reopen the NEW shared handle on its device.
    surface.angle_shared_texture = None;
    Ok(())
}

/// Engine releases a backing store. The boxed [`GlBackingStore`] is reclaimed by
/// the framebuffer destruction callback; this collect hook only needs to succeed.
extern "C" fn collect_backing_store_callback(
    _renderer: *const FlutterBackingStore,
    _user_data: *mut c_void,
) -> bool {
    true
}

/// Engine has composited all layers of `info.view_id`. Publish the frame to the
/// host: flush GL, capture damage, and bump the per-view frame counter so the
/// host copy step picks it up. Satellite views have no keyed mutex (it does not
/// work across the three device round-trips), so there is no mutex to release —
/// the bumped frame counter is the publish signal.
extern "C" fn present_view_callback(info: *const FlutterPresentViewInfo) -> bool {
    if info.is_null() {
        error!("[compositor] present_view: null info");
        return false;
    }
    let info = unsafe { &*info };
    if info.user_data.is_null() {
        error!("[compositor] present_view: null user_data");
        return false;
    }
    let host = unsafe { &*(info.user_data as *mut FlutterOverlay) };
    let view_id = info.view_id;

    if view_id == IMPLICIT_VIEW_ID {
        return present_implicit_view(host, info);
    }

    let presented = host.view_registry.with_view(view_id, |surface| {
        // Ensure GL work is submitted before the host reads the shared texture.
        surface.gl_flush();

        // Record damage for this frame (used by the host partial-copy step).
        // FlutterPresentViewInfo carries layers, not damage rects directly; the
        // backing-store present info holds per-layer damage. Capture it here.
        record_damage(surface, info);

        // No keyed mutex on satellite views (it does not work across the three
        // device round-trips), so there is nothing to release. The frame counter
        // bump below is the sole publish signal to the host.
        surface.frame_presented.fetch_add(1, Ordering::Release);
        surface.signal_frame();
        true
    });

    match presented {
        Some(true) => true,
        Some(false) => false,
        None => {
            error!("[compositor] present_view: no view surface for view_id {view_id}");
            false
        }
    }
}

/// Presents the implicit view (view 0, the in-game overlay) through the
/// compositor path. Mirrors the legacy `present_with_info_callback`: flush GL,
/// capture frame damage into the overlay's own buffers, release the overlay's
/// keyed mutex (key 1 = game may read), and bump `angle_frame_presented`.
fn present_implicit_view(host: &FlutterOverlay, info: &FlutterPresentViewInfo) -> bool {
    // Flush via the view-0 GL procs if available.
    if let Some(gl) = &host.view0_gl {
        unsafe { (gl.procs.flush)() };
    }

    // Frame damage → drained by tick() for partial copy from the shared texture.
    collect_implicit_frame_damage(host, info);

    // If the compositor reported no per-layer damage, force a full-frame copy so
    // tick() doesn't skip the frame (which would leave the overlay blank). The
    // legacy present_with_info path always produced damage; the compositor may
    // not, so we synthesize a full rect here.
    if let Ok(mut rects) = host.frame_damage_rects.lock()
        && rects.is_empty()
    {
        rects.push(FlutterRect {
            left: 0.0,
            top: 0.0,
            right: host.width as f64,
            bottom: host.height as f64,
        });
    }

    if let Some(mutex) = &host.angle_keyed_mutex {
        let _ = unsafe { mutex.ReleaseSync(1) };
    }

    host.angle_frame_presented.fetch_add(1, Ordering::Release);
    true
}

/// Accumulates per-layer paint regions for the implicit view into the overlay's
/// `frame_damage_rects`, matching the satellite-view `record_damage` shape.
fn collect_implicit_frame_damage(host: &FlutterOverlay, info: &FlutterPresentViewInfo) {
    if info.layers.is_null() || info.layers_count == 0 {
        return;
    }
    let layers = unsafe { std::slice::from_raw_parts(info.layers, info.layers_count) };
    if let Ok(mut rects) = host.frame_damage_rects.lock() {
        for layer_ptr in layers {
            if layer_ptr.is_null() {
                continue;
            }
            let layer = unsafe { &**layer_ptr };
            if layer.backing_store_present_info.is_null() {
                continue;
            }
            let pi = unsafe { &*layer.backing_store_present_info };
            if pi.paint_region.is_null() {
                continue;
            }
            let region = unsafe { &*pi.paint_region };
            if region.rects_count == 0 || region.rects.is_null() {
                continue;
            }
            let slice = unsafe { std::slice::from_raw_parts(region.rects, region.rects_count) };
            rects.extend_from_slice(slice);
        }
    }
}

/// Extracts per-layer backing-store damage from the present info and stores it on
/// the view surface for the host's partial-copy step.
fn record_damage(surface: &mut ViewSurface, info: &FlutterPresentViewInfo) {
    if info.layers.is_null() || info.layers_count == 0 {
        return;
    }
    let layers = unsafe { std::slice::from_raw_parts(info.layers, info.layers_count) };

    if let Ok(mut rects) = surface.frame_damage_rects.lock() {
        for layer_ptr in layers {
            if layer_ptr.is_null() {
                continue;
            }
            let layer = unsafe { &**layer_ptr };
            let present_info = layer.backing_store_present_info;
            if present_info.is_null() {
                continue;
            }
            let pi = unsafe { &*present_info };
            if pi.paint_region.is_null() {
                continue;
            }
            let region = unsafe { &*pi.paint_region };
            if region.rects_count == 0 || region.rects.is_null() {
                continue;
            }
            let rects_slice =
                unsafe { std::slice::from_raw_parts(region.rects, region.rects_count) };
            rects.extend_from_slice(rects_slice);
        }
    }
}
