use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, Once};
use std::time::Instant;

use log::{error, info, warn};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
};
use windows::Win32::Graphics::Dxgi::{DXGI_SWAP_CHAIN_DESC, IDXGISwapChain};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCMOUSELEAVE, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

use crate::init_logging;
use crate::software_renderer::api::{FlutterEmbedderError, RendererType};
use crate::software_renderer::d3d11_compositor::effects::{
    EffectConfig, EffectParams, EffectTarget, HologramParams, PostEffect, WarpFieldParams,
};

use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;
use crate::software_renderer::overlay::semantics_handler::update_interactive_widget_hover_state;

/// A thread-safe, clonable handle for interacting with the global OverlayManager.
#[derive(Clone, Copy)]
pub struct FlutterOverlayManagerHandle {
    pub manager: &'static Mutex<OverlayManager>,
}

/// Gets a thread-safe handle to the global OverlayManager.
///
/// This handle is lightweight and can be cloned and passed between threads.
pub fn get_flutter_overlay_manager_handle() -> Option<FlutterOverlayManagerHandle> {
    get_overlay_manager().map(|manager_mutex| FlutterOverlayManagerHandle {
        manager: manager_mutex,
    })
}

static OVERLAY_MANAGER: Once = Once::new();
static mut GLOBAL_OVERLAY_MANAGER: Option<Mutex<OverlayManager>> = None;

pub trait FlutterPainter {
    /// Paint the given Flutter texture with the main scene.
    fn paint_texture(
        &mut self,
        texture_srv: &ID3D11ShaderResourceView,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    );
}

/// Provides access to the global `OverlayManager` singleton.
fn get_overlay_manager() -> Option<&'static Mutex<OverlayManager>> {
    unsafe {
        OVERLAY_MANAGER.call_once(|| {
            GLOBAL_OVERLAY_MANAGER = Some(Mutex::new(OverlayManager::new()));
        });
        GLOBAL_OVERLAY_MANAGER.as_ref()
    }
}

pub struct OverlayManager {
    /// Stores the actual FlutterOverlay instances, keyed by a unique identifier.
    pub active_instances: HashMap<String, Box<FlutterOverlay>>,
    /// Defines the rendering and input priority. The last element is considered topmost.
    pub overlay_order: Vec<String>,
    /// Identifier of the overlay that currently has keyboard focus.
    pub focused_overlay_id: Option<String>,
    /// Shared Direct3D device context for ticking overlays.
    shared_d3d_context: Option<ID3D11DeviceContext>,
    /// The width of the screen in pixels.
    screen_width: u32,
    /// The height of the screen in pixels.
    screen_height: u32,
    /// The time when the `OverlayManager` was created or resumed.
    start_time: Instant,
    /// Indicates whether the `OverlayManager` is currently paused.
    is_paused: bool,
    /// The time in seconds when the `OverlayManager` was paused.
    time_at_pause: f32,
}

impl OverlayManager {
    /// Creates a new, empty `OverlayManager`.
    fn new() -> Self {
        OverlayManager {
            active_instances: HashMap::new(),
            overlay_order: Vec::new(),
            focused_overlay_id: None,
            shared_d3d_context: None,
            screen_width: 0,
            screen_height: 0,
            start_time: Instant::now(),
            is_paused: false,
            time_at_pause: 0.0,
        }
    }
    /// Checks if the overlay identified by `identifier` currently has keyboard focus.
    pub fn is_focused(&self, identifier: &str) -> bool {
        self.focused_overlay_id.as_deref() == Some(identifier)
    }
    /// Retrieves the dimensions (width, height) for all active overlays.
    ///
    /// # Returns
    ///
    /// A `HashMap` where keys are overlay identifiers and values are tuples
    /// containing the width and height of the overlay.
    pub fn get_all_overlay_dimensions(&self) -> HashMap<String, (u32, u32)> {
        self.active_instances
            .iter()
            .map(|(id, overlay)| (id.clone(), overlay.get_dimensions()))
            .collect()
    }
    /// Sets the visibility of a specific overlay instance.
    ///
    /// If the overlay with the given `identifier` does not exist, a warning is logged.
    ///
    /// # Arguments
    ///
    /// * `identifier` - The unique identifier of the overlay.
    /// * `is_visible` - A boolean indicating whether the overlay should be visible (`true`)
    ///                  or hidden (`false`).
    pub fn set_overlay_visibility(&mut self, identifier: &str, is_visible: bool) {
        if let Some(overlay) = self.active_instances.get_mut(identifier) {
            overlay.set_visibility(is_visible);
        } else {
            warn!(
                "[OverlayManager] Attempted to set visibility for unknown overlay '{}'.",
                identifier
            );
        }
    }

    /// Registers a Dart port for a specific overlay instance.
    pub fn register_dart_port(&self, identifier: &str, port: i64) {
        if let Some(overlay) = self.active_instances.get(identifier) {
            overlay.register_dart_port(port);
        } else {
            warn!(
                "[OverlayManager] Attempted to register port for unknown overlay '{}'.",
                identifier
            );
        }
    }

    /// Posts a boolean message to a specific overlay instance.
    pub fn post_bool_to_overlay(
        &self,
        identifier: &str,
        value: bool,
    ) -> Result<(), FlutterEmbedderError> {
        if let Some(overlay) = self.active_instances.get(identifier) {
            overlay.post_bool(value)
        } else {
            Err(FlutterEmbedderError::InvalidHandle)
        }
    }

    /// Posts an i64 message to a specific overlay instance.
    pub fn post_i64_to_overlay(
        &self,
        identifier: &str,
        value: i64,
    ) -> Result<(), FlutterEmbedderError> {
        if let Some(overlay) = self.active_instances.get(identifier) {
            overlay.post_i64(value)
        } else {
            Err(FlutterEmbedderError::InvalidHandle)
        }
    }

    /// Posts an f64 message to a specific overlay instance.
    pub fn post_f64_to_overlay(
        &self,
        identifier: &str,
        value: f64,
    ) -> Result<(), FlutterEmbedderError> {
        if let Some(overlay) = self.active_instances.get(identifier) {
            overlay.post_f64(value)
        } else {
            Err(FlutterEmbedderError::InvalidHandle)
        }
    }

    /// Posts a string message to a specific overlay instance.
    pub fn post_string_to_overlay(
        &self,
        identifier: &str,
        value: &str,
    ) -> Result<(), FlutterEmbedderError> {
        if let Some(overlay) = self.active_instances.get(identifier) {
            overlay.post_string(value)
        } else {
            Err(FlutterEmbedderError::InvalidHandle)
        }
    }

    /// Posts a byte buffer to a specific overlay instance.
    pub fn post_buffer_to_overlay(
        &self,
        identifier: &str,
        buffer: &[u8],
    ) -> Result<(), FlutterEmbedderError> {
        if let Some(overlay) = self.active_instances.get(identifier) {
            overlay.post_buffer(buffer)
        } else {
            Err(FlutterEmbedderError::InvalidHandle)
        }
    }

    /// Sets the screen-space position for a specific overlay.
    pub fn set_overlay_position(&mut self, identifier: &str, x: i32, y: i32) {
        if let Some(overlay) = self.active_instances.get_mut(identifier) {
            overlay.set_position(x, y);
        } else {
            warn!(
                "[OverlayManager] Attempted to set position for unknown overlay '{}'.",
                identifier
            );
        }
    }

    /// Finds the topmost, visible overlay that contains the given screen coordinates.
    pub fn find_topmost_overlay_at_position(&self, x: i32, y: i32) -> Option<String> {
        for identifier in self.overlay_order.iter().rev() {
            if let Some(overlay) = self.active_instances.get(identifier) {
                if !overlay.is_visible() {
                    continue;
                }
                let (ox, oy) = overlay.get_position();
                let (ow, oh) = overlay.get_dimensions();
                if (x >= ox && x < (ox + ow as i32)) && (y >= oy && y < (oy + oh as i32)) {
                    return Some(identifier.clone());
                }
            }
        }
        None
    }

    pub fn get_d3d_context(&self) -> Option<ID3D11DeviceContext> {
        self.shared_d3d_context.clone()
    }

    /// Internal helper to add an overlay instance and manage its order and focus.
    fn add_overlay_instance(&mut self, identifier: String, overlay_box: Box<FlutterOverlay>) {
        if self.active_instances.contains_key(&identifier) {
            warn!(
                "[OverlayManager] Overlay with identifier '{}' already exists. It will be replaced and brought to front.",
                identifier
            );
            if let Some(old_overlay) = self.active_instances.remove(&identifier) {
                if let Err(e) = old_overlay.shutdown() {
                    error!(
                        "[OverlayManager] Error shutting down old overlay instance '{}' during replacement: {}",
                        identifier, e
                    );
                }
            }
            self.overlay_order.retain(|id| id != &identifier);
        }

        self.active_instances
            .insert(identifier.clone(), overlay_box);
        self.overlay_order.push(identifier.clone());

        if self.focused_overlay_id.is_none() {
            self.focused_overlay_id = Some(identifier.clone());
        }
    }

    /// Initializes a new Flutter overlay instance.
    fn init(
        &mut self,
        swap_chain: &IDXGISwapChain,
        flutter_asset_dir: &PathBuf,
        identifier: &str,
        dart_args_for_this_instance: Option<Vec<String>>,
        engine_args_opt: Option<Vec<String>>,
    ) -> bool {
        if self.active_instances.contains_key(identifier) {
            self.bring_to_front(identifier);
            // self.set_keyboard_focus(identifier);
            return true;
        }

        let device = match unsafe { swap_chain.GetDevice::<ID3D11Device>() } {
            Ok(d) => d,
            Err(e) => {
                error!(
                    "[OverlayManager:{}] Failed to get D3D11 Device from swap chain: {:?}",
                    identifier, e
                );
                return false;
            }
        };

        if self.shared_d3d_context.is_none() {
            match unsafe { device.GetImmediateContext() } {
                Ok(ctx) => self.shared_d3d_context = Some(ctx),
                Err(e) => {
                    error!(
                        "[OverlayManager:{}] Failed to get D3D11 Immediate Context: {:?}",
                        identifier, e
                    );
                    return false;
                }
            }
        }

        let get_desc_result: windows::core::Result<DXGI_SWAP_CHAIN_DESC> =
            unsafe { swap_chain.GetDesc() };

        let desc: DXGI_SWAP_CHAIN_DESC;

        match get_desc_result {
            Ok(d) => {
                desc = d;
            }
            Err(e) => {
                error!(
                    "[OverlayManager:{}] Failed to get SwapChain description: {:?}",
                    identifier, e
                );
                return false;
            }
        }

        let width = desc.BufferDesc.Width;
        let height = desc.BufferDesc.Height;

        self.screen_width = width;
        self.screen_height = height;

        init_logging();

        match FlutterOverlay::create(
            identifier.to_string(),
            &device,
            swap_chain,
            0,
            0,
            width,
            height,
            flutter_asset_dir.clone(),
            dart_args_for_this_instance,
            engine_args_opt,
        ) {
            Ok(overlay_box) => {
                self.add_overlay_instance(identifier.to_string(), overlay_box);
                info!(
                    "[OverlayManager:{}] Flutter overlay initialized and added to manager.",
                    identifier
                );
                true
            }
            Err(e) => {
                error!(
                    "[OverlayManager:{}] Failed to create FlutterOverlay instance: {:?}",
                    identifier, e
                );
                false
            }
        }
    }

    /// Brings the specified overlay to the top of the Z-order.
    pub fn bring_to_front(&mut self, identifier: &str) {
        if self.active_instances.contains_key(identifier) {
            self.overlay_order.retain(|id| id != identifier);
            self.overlay_order.push(identifier.to_string());
        }
    }

    /// Sets keyboard focus to the specified overlay and brings it to the front.
    pub fn set_keyboard_focus(&mut self, identifier: &str) {
        if self.active_instances.contains_key(identifier) {
            self.focused_overlay_id = Some(identifier.to_string());
            self.bring_to_front(identifier);
        }
    }

    /// Runs the per-frame logic for all active overlays.
    fn run(&mut self) {
        if self.active_instances.is_empty() {
            return;
        }

        for id in &self.overlay_order {
            if let Some(overlay) = self.active_instances.get(id) {
                if !overlay.is_visible() {
                    continue;
                }

                overlay.request_frame().ok();

                if overlay.renderer_type == RendererType::OpenGL {
                    // TODO: Handle opengl renderer
                }
            }
        }
    }

    /// Handles input events, routing them based on Z-order and focus.
    fn handle_input_event(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
        if self.active_instances.is_empty() {
            return false;
        }

        let is_pointer_event = matches!(
            msg,
            WM_MOUSEMOVE
                | WM_LBUTTONDOWN
                | WM_RBUTTONDOWN
                | WM_MBUTTONDOWN
                | WM_LBUTTONUP
                | WM_RBUTTONUP
                | WM_MBUTTONUP
                | WM_NCMOUSELEAVE
                | WM_MOUSEWHEEL
        );

        let is_key_event = matches!(
            msg,
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP | WM_CHAR
        );

        if is_pointer_event {
            let overlay_order_copy: Vec<String> = self.overlay_order.clone();

            for identifier in overlay_order_copy.iter().rev() {
                if let Some(overlay_instance) = self.active_instances.get(identifier) {
                    if !overlay_instance.is_visible() {
                        continue;
                    }

                    overlay_instance.handle_pointer_event(hwnd, msg, wparam, lparam);

                    if overlay_instance
                        .is_interactive_widget_hovered
                        .load(Ordering::SeqCst)
                    {
                        self.bring_to_front(identifier);
                        return true;
                    }
                }
            }
        } else if is_key_event {
            if let Some(focused_id) = &self.focused_overlay_id {
                if let Some(overlay_instance) = self.active_instances.get(focused_id) {
                    if overlay_instance.is_visible() {
                        if overlay_instance.handle_keyboard_event(msg, wparam, lparam) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Handles WM_SETCURSOR, respecting Z-order and hover states.
    fn handle_set_cursor(
        &self,
        hwnd_for_setcursor_message: HWND,
        lparam_from_message: LPARAM,
        main_app_hwnd: HWND,
    ) -> Option<LRESULT> {
        for identifier in self.overlay_order.iter().rev() {
            // Topmost first
            if let Some(overlay_instance) = self.active_instances.get(identifier) {
                if overlay_instance
                    .is_interactive_widget_hovered
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    if let Some(lresult) = overlay_instance.handle_set_cursor(
                        hwnd_for_setcursor_message,
                        lparam_from_message,
                        main_app_hwnd,
                    ) {
                        return Some(lresult);
                    }
                }
            }
        }
        None
    }

    /// Handles resizing for all active overlays.
    fn handle_resize(
        &mut self,
        swap_chain: &IDXGISwapChain,
        x_pos: i32,
        y_pos: i32,
        width: u32,
        height: u32,
    ) {
        self.screen_width = width;
        self.screen_height = height;

        if self.active_instances.is_empty() {
            return;
        }

        for (id, overlay_instance) in self.active_instances.iter_mut() {
            if !overlay_instance.engine.0.is_null() {
                overlay_instance.handle_window_resize(x_pos, y_pos, width, height, &swap_chain);
            } else {
                warn!(
                    "[OverlayManager:{}] Engine handle is null, cannot resize.",
                    id
                );
            }
        }
    }

    /// Shuts down a specific Flutter overlay instance.
    fn shutdown_instance(&mut self, identifier: &str) -> Result<(), FlutterEmbedderError> {
        if let Some(overlay_box) = self.active_instances.remove(identifier) {
            info!(
                "[OverlayManager:{}] Shutting down overlay instance.",
                identifier
            );
            self.overlay_order.retain(|id| id != identifier);

            if self.focused_overlay_id.as_deref() == Some(identifier) {
                self.focused_overlay_id = self.overlay_order.last().cloned();
            }
            overlay_box.shutdown()
        } else {
            warn!(
                "[OverlayManager:{}] Shutdown called for unknown or already removed instance.",
                identifier
            );
            Ok(())
        }
    }

    /// Shuts down all active Flutter overlay instances.
    pub fn shutdown_all_instances(&mut self) {
        let all_ids: Vec<String> = self.active_instances.keys().cloned().collect();

        for id in all_ids {
            if let Err(e) = self.shutdown_instance(&id) {
                error!(
                    "[OverlayManager] Error during shutdown of instance {}: {}",
                    id, e
                );
            }
        }

        info!("[OverlayManager] All instances shut down.");
    }

    /// Sends the same platform message to all visible overlay instances.
    /// This is useful for broadcasting global events.
    pub fn broadcast_platform_message(&self, channel: &str, message: &[u8]) {
        for (id, overlay) in self.active_instances.iter() {
            if !overlay.is_visible() {
                continue;
            }

            if let Err(e) = overlay.send_platform_message(channel, message) {
                error!(
                    "[OverlayManager] Failed to broadcast message to overlay '{}': {}",
                    id, e
                );
            }
        }
    }

    // Get all texture to process them before calling tick allowing special effects or resizing.
    pub fn get_all_overlay_textures(&self) -> Vec<(String, ID3D11ShaderResourceView)> {
        let mut textures = Vec::new();

        for identifier in &self.overlay_order {
            if let Some(overlay) = self.active_instances.get(identifier) {
                if overlay.is_visible() {
                    if let Ok(texture_srv) = overlay.get_texture_srv() {
                        textures.push((identifier.clone(), texture_srv));
                    }
                }
            }
        }

        textures
    }
}

impl FlutterOverlayManagerHandle {
    /// Creates and initializes a new Flutter overlay instance and adds it to the manager.
    ///
    /// This function is the entry point for creating a new Flutter UI surface. It handles
    /// loading the Flutter engine, preparing rendering resources, and running the Dart
    /// isolate. If an overlay with the same `identifier` exists, it is shut down and
    /// replaced by the new instance.
    ///
    /// # Arguments
    /// * `swap_chain`: A reference to the host application's `IDXGISwapChain`.
    /// * `flutter_asset_build_dir`: The file path to the Flutter project's assets directory,
    ///   which is `Debug or Release` in the Flutter app's build output of windows or others..
    ///   The Debug or Release dir needs to contain the flutter_engine.dll/lib JIT or AOT build since it get's loaded dynamically.
    /// * `identifier`: A unique string like "flutter_{any name}" that identifies this overlay instance for all
    ///   subsequent API calls.
    /// * `dart_args`: Optional. A vector of string arguments for the Dart `main()` function.
    /// * `engine_args`: Optional. A vector of command-line switches for the Flutter Engine used in Debug JIT.
    ///
    /// # Returns
    /// Returns `true` if the overlay was initialized successfully. Returns `false` if an
    /// error occurred, which will be logged internally.
    pub fn init_instance(
        &self,
        swap_chain: &IDXGISwapChain,
        flutter_asset_build_dir: &PathBuf,
        identifier: &str,
        dart_args: Option<Vec<String>>,
        engine_args: Option<Vec<String>>,
    ) -> bool {
        if let Ok(mut manager) = self.manager.lock() {
            manager.init(
                swap_chain,
                flutter_asset_build_dir,
                identifier,
                dart_args,
                engine_args,
            )
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for init_instance.");
            false
        }
    }

    /// Schedules a new frame request for all visible Flutter overlays.
    /// This should be called at the beginning of a frame or end of the previous one
    /// to give Flutter maximum time to render.
    pub fn schedule_flutter_frames(&self) {
        if let Ok(manager) = self.manager.lock() {
            for id in &manager.overlay_order {
                if let Some(overlay) = manager.active_instances.get(id) {
                    if overlay.is_visible() {
                        // This just tells the engine to start working on a new frame.
                        overlay.request_frame().ok();
                    }
                }
            }
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for schedule_flutter_frames.");
        }
    }

    /// Ticks and composites all visible overlays onto the screen.
    /// This should be called late in the frame, just before Present.
    pub fn composite_flutter_overlays(&self) {
        if let Ok(manager) = self.manager.lock() {
            let context = match manager.shared_d3d_context.clone() {
                Some(ctx) => ctx,
                None => return,
            };
            let time = manager.start_time.elapsed().as_secs_f32();

            for id in &manager.overlay_order {
                if let Some(overlay) = manager.active_instances.get(id) {
                    if !overlay.is_visible() {
                        continue;
                    }

                    if overlay.renderer_type == RendererType::Software {
                        overlay.tick(&context);
                    }

                    update_interactive_widget_hover_state(overlay);

                    // This draws the most recently completed frame from Flutter.
                    overlay.composite(&context, manager.screen_width, manager.screen_height, time);
                }
            }
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for tick compositing.");
        }
    }

    /// Updates the screen dimensions used by the overlays.
    /// This must be called when the main window is resized.
    pub fn update_screen_size(&self, width: u32, height: u32) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.screen_width = width;
            manager.screen_height = height;
        }
    }

    /// Pauses all shader animations for all overlays.
    pub fn pause_animations(&self) {
        if let Ok(mut manager) = self.manager.lock() {
            if !manager.is_paused {
                manager.time_at_pause = manager.start_time.elapsed().as_secs_f32();
                manager.is_paused = true;
            }
        }
    }

    /// Resumes all shader animations for all overlays.
    pub fn resume_animations(&self) {
        if let Ok(mut manager) = self.manager.lock() {
            if manager.is_paused {
                manager.start_time =
                    Instant::now() - std::time::Duration::from_secs_f32(manager.time_at_pause);
                manager.is_paused = false;
            }
        }
    }

    /// Sets a post-processing effect for the **entire** overlay.
    pub fn set_fullscreen_effect(&self, identifier: &str, effect: PostEffect) {
        if let Ok(mut manager) = self.manager.lock() {
            if let Some(overlay) = manager.active_instances.get_mut(identifier) {
                overlay.effect_config.params = match effect {
                    PostEffect::Passthrough => EffectParams::None,
                    PostEffect::Hologram => EffectParams::Hologram(HologramParams::default()),
                    PostEffect::WarpField => EffectParams::WarpField(WarpFieldParams::default()),
                    //...
                };
                overlay.effect_config.target = EffectTarget::Fullscreen;
            }
        }
    }

    // TODO: Make it not by widget - make it by widgets []
    /// Applies a post-processing effect to a **specific area** of an overlay.
    pub fn set_widget_effect(&self, identifier: &str, effect: PostEffect, bounds: [f32; 4]) {
        if let Ok(mut manager) = self.manager.lock() {
            if let Some(overlay) = manager.active_instances.get_mut(identifier) {
                overlay.effect_config.params = match effect {
                    PostEffect::Passthrough => EffectParams::None,
                    PostEffect::Hologram => EffectParams::Hologram(HologramParams::default()),
                    PostEffect::WarpField => EffectParams::WarpField(WarpFieldParams::default()),
                    //...
                };
                overlay.effect_config.target = EffectTarget::Widget(bounds);
            }
        }
    }

    /// Removes any active effect from an overlay, reverting it to the default passthrough shader.
    pub fn clear_effect(&self, identifier: &str) {
        if let Ok(mut manager) = self.manager.lock() {
            if let Some(overlay) = manager.active_instances.get_mut(identifier) {
                overlay.effect_config = EffectConfig::default();
            }
        }
    }

    /// Updates the entire effect configuration for a specific overlay.
    /// This is the single, powerful entry point for controlling an overlay's visual style.
    ///
    /// # Arguments
    /// * `identifier`: The unique name of the overlay to configure.
    /// * `config`: The complete `EffectConfig` struct defining the target area
    ///   and the specific effect parameters to apply.
    pub fn update_effect_config(&self, identifier: &str, config: EffectConfig) {
        if let Ok(mut manager) = self.manager.lock() {
            if let Some(overlay) = manager.active_instances.get_mut(identifier) {
                overlay.effect_config = config;
            }
        }
    }

    /// Forwards a raw Windows message to the manager for input processing.
    ///
    /// The manager routes the event to the appropriate overlay based on Z-order for
    /// pointer events and keyboard focus for key events. This function is the primary
    /// mechanism for delivering user input to the Flutter UIs.
    ///
    /// # Usage
    /// This function must be called from the host application's `WndProc` for all
    /// relevant input messages (e.g., `WM_MOUSEMOVE`, `WM_KEYDOWN`, `WM_CHAR`).
    ///
    /// # Returns
    /// Returns `true` if a Flutter overlay consumed the event. The host application can
    /// use this to suppress further processing of the input. Returns `false` otherwise.
    pub fn forward_input_to_flutter(
        &self,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> bool {
        if let Ok(mut manager) = self.manager.lock() {
            manager.handle_input_event(hwnd, msg, wparam, lparam)
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for forward_input_to_flutter.");
            false
        }
    }

    /// Requests that the topmost active overlay under the cursor set the mouse cursor style.
    ///
    /// # Usage
    /// This function must be called from the host application's `WndProc` when handling
    /// the `WM_SETCURSOR` message.
    ///
    /// # Returns
    /// * `Some(LRESULT(1))` if a Flutter overlay handled the cursor request. The `WndProc`
    ///   should return this value to prevent default Windows cursor handling.
    /// * `None` if no overlay handled the request.
    pub fn set_flutter_cursor(
        &self,
        hwnd_for_setcursor_message: HWND,
        lparam: LPARAM,
        original_hwnd: HWND,
    ) -> Option<LRESULT> {
        if let Ok(manager) = self.manager.lock() {
            manager.handle_set_cursor(hwnd_for_setcursor_message, lparam, original_hwnd)
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for set_flutter_cursor.");
            None
        }
    }

    /// Notifies all active overlays of a window or render area resize.
    ///
    /// This call updates the logical and physical dimensions for each Flutter instance
    /// and regenerates its underlying GPU texture to match the new size.
    ///
    /// # Usage
    /// Call this function when the main window is resized or the D3D11 swap chain is recreated.
    pub fn resize_flutter_overlays(
        &self,
        swap_chain: &IDXGISwapChain,
        x_pos: i32,
        y_pos: i32,
        width: u32,
        height: u32,
    ) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.handle_resize(swap_chain, x_pos, y_pos, width, height);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for resize_flutter_overlays.");
        }
    }

    /// Shuts down a specific Flutter overlay instance, releasing all its resources.
    /// The overlay identified by the `identifier` can no longer be used after this call.
    pub fn shutdown_instance(&self, identifier: &str) {
        if let Ok(mut manager) = self.manager.lock() {
            if let Err(e) = manager.shutdown_instance(identifier) {
                error!(
                    "[OverlayManagerHandle] Error during shutdown of instance {}: {}",
                    identifier, e
                );
            }
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for shutdown_instance.");
        }
    }

    /// Shuts down all currently active Flutter overlay instances.
    pub fn shutdown_all_instances(&self) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.shutdown_all_instances();
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for shutdown_all_instances.");
        }
    }

    /// Brings the specified overlay to the top of the rendering order (Z-order).
    /// The identified overlay will be drawn on top of all other overlays.
    pub fn bring_to_front(&self, identifier: &str) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.bring_to_front(identifier);
            manager.set_keyboard_focus(identifier);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for bring_to_front.");
        }
    }

    /// Sets keyboard focus to the specified overlay.
    ///
    /// The focused overlay will receive all subsequent keyboard input. This action also
    /// brings the overlay to the front of the rendering order.
    pub fn set_focus(&self, identifier: &str) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.set_keyboard_focus(identifier);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for set_focus.");
        }
    }

    /// Checks if the overlay with the given identifier currently has keyboard focus.
    pub fn is_focused(&self, identifier: &str) -> bool {
        self.manager
            .lock()
            .map_or(false, |manager| manager.is_focused(identifier))
    }

    /// Sets the visibility of a specific Flutter overlay.
    /// An invisible overlay is not rendered and does not receive input.
    pub fn set_visibility(&self, identifier: &str, is_visible: bool) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.set_overlay_visibility(identifier, is_visible);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for set_visibility.");
        }
    }

    /// Sets the screen-space position of an overlay's top-left corner.
    pub fn set_position(&self, identifier: &str, x: i32, y: i32) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.set_overlay_position(identifier, x, y);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for set_position.");
        }
    }

    /// Sends a platform message to all visible overlays.
    ///
    /// # Note
    /// For new development, prefer the `post_*` methods for high-performance,
    /// one-way messaging.
    pub fn broadcast_message(&self, channel: &str, message: &[u8]) {
        if let Ok(manager) = self.manager.lock() {
            manager.broadcast_platform_message(channel, message);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for broadcast_message.");
        }
    }

    /// Gets the dimensions (width, height) of all active overlays.
    pub fn get_all_dimensions(&self) -> HashMap<String, (u32, u32)> {
        self.manager.lock().map_or(HashMap::new(), |manager| {
            manager.get_all_overlay_dimensions()
        })
    }

    /// Gets a clone of the shared Direct3D device context used by the manager.
    pub fn get_d3d_context(&self) -> Option<ID3D11DeviceContext> {
        self.manager
            .lock()
            .ok()
            .and_then(|manager| manager.get_d3d_context())
    }

    /// Finds the identifier of the topmost, visible overlay at a given screen coordinate.
    /// This is used for hit-testing to determine which overlay is under the mouse.
    pub fn find_at_position(&self, x: i32, y: i32) -> Option<String> {
        self.manager
            .lock()
            .ok()
            .and_then(|manager| manager.find_topmost_overlay_at_position(x, y))
    }

    /// Registers a Dart `SendPort` with a specific overlay instance.
    ///
    /// This is a required setup step for using the `post_*_to_overlay` methods.
    /// The Dart application must create a `ReceivePort`, get its `sendPort.nativePort`,
    /// and send the resulting `i64` handle to this function via an FFI call.
    pub fn register_dart_port(&self, identifier: &str, port: i64) {
        if let Ok(manager) = self.manager.lock() {
            manager.register_dart_port(identifier, port);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for register_dart_port.");
        }
    }

    /// Sends a boolean message to a single overlay identified by its name.
    pub fn post_bool_to_overlay(&self, identifier: &str, value: bool) -> bool {
        if let Ok(manager) = self.manager.lock() {
            manager.post_bool_to_overlay(identifier, value).is_ok()
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for post_bool_to_overlay.");
            false
        }
    }

    /// Sends an i64 message to a single overlay identified by its name.
    pub fn post_i64_to_overlay(&self, identifier: &str, value: i64) -> bool {
        if let Ok(manager) = self.manager.lock() {
            manager.post_i64_to_overlay(identifier, value).is_ok()
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for post_i64_to_overlay.");
            false
        }
    }

    /// Sends an f64 message to a single overlay identified by its name.
    pub fn post_f64_to_overlay(&self, identifier: &str, value: f64) -> bool {
        if let Ok(manager) = self.manager.lock() {
            manager.post_f64_to_overlay(identifier, value).is_ok()
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for post_f64_to_overlay.");
            false
        }
    }

    /// Sends a string message to a single overlay identified by its name.
    pub fn post_string_to_overlay(&self, identifier: &str, value: &str) -> bool {
        if let Ok(manager) = self.manager.lock() {
            manager.post_string_to_overlay(identifier, value).is_ok()
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for post_string_to_overlay.");
            false
        }
    }

    /// Sends a byte buffer to a single overlay identified by its name.
    pub fn post_buffer_to_overlay(&self, identifier: &str, buffer: &[u8]) -> bool {
        if let Ok(manager) = self.manager.lock() {
            manager.post_buffer_to_overlay(identifier, buffer).is_ok()
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for post_buffer_to_overlay.");
            false
        }
    }

    /// Retrieves the rendered textures from all active and visible overlays.
    pub fn get_all_overlay_textures(&self) -> Vec<(String, ID3D11ShaderResourceView)> {
        if let Ok(manager) = self.manager.lock() {
            manager.get_all_overlay_textures()
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for get_all_overlay_textures.");
            Vec::new()
        }
    }
}
