use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Once;
use std::sync::atomic::Ordering;
use std::time::Instant;

use directx_math::XMMatrix;
use log::{error, info, warn};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
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
use crate::software_renderer::api::FlutterEmbedderError;
use crate::software_renderer::d3d11_compositor::effects::{
    EffectConfig, EffectParams, EffectTarget, HologramParams, PostEffect, WarpFieldParams,
};

use crate::software_renderer::d3d11_compositor::primitive_3d_renderer::{PrimitiveType, Vertex3D};
use crate::software_renderer::d3d11_compositor::scoped_render_state::ScopedRenderState;
use crate::software_renderer::d3d11_compositor::traits::{FrameParams, Renderer};
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

    /// Gets an immutable reference to a target overlay.
    ///
    /// If `identifier` is `None`, it attempts to get the single active instance.
    fn get_instance(&self, identifier: Option<&str>) -> Result<&Box<FlutterOverlay>, String> {
        match identifier {
            Some(id) => self
                .active_instances
                .get(id)
                .ok_or_else(|| format!("No overlay with identifier '{}' found.", id)),
            None => {
                if self.active_instances.len() == 1 {
                    Ok(self.active_instances.values().next().unwrap())
                } else if self.active_instances.is_empty() {
                    Err("No active overlay instance found.".to_string())
                } else {
                    Err("Multiple overlays exist; an identifier is required.".to_string())
                }
            }
        }
    }

    /// Gets a mutable reference to a target overlay.
    ///
    /// If `identifier` is `None`, it attempts to get the single active instance.
    fn get_instance_mut(
        &mut self,
        identifier: Option<&str>,
    ) -> Result<&mut Box<FlutterOverlay>, String> {
        match identifier {
            Some(id) => self
                .active_instances
                .get_mut(id)
                .ok_or_else(|| format!("No overlay with identifier '{}' found.", id)),
            None => {
                if self.active_instances.len() == 1 {
                    Ok(self.active_instances.values_mut().next().unwrap())
                } else if self.active_instances.is_empty() {
                    Err("No active overlay instance found.".to_string())
                } else {
                    Err("Multiple overlays exist; an identifier is required.".to_string())
                }
            }
        }
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

    pub fn latch_all_queued_primitives(&mut self) {
        for overlay in self.active_instances.values_mut() {
            overlay.latch_queued_primitives();
        }
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
            self.bring_to_front(Some(identifier));
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
                        self.bring_to_front(Some(identifier));
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

    /// Checks if the specified overlay currently has keyboard focus.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    pub fn is_focused(&self, identifier: Option<&str>) -> bool {
        if let Ok(overlay) = self.get_instance(identifier) {
            self.focused_overlay_id.as_deref() == Some(overlay.name.as_str())
        } else {
            false
        }
    }

    /// Sets the visibility of a specific overlay instance.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `is_visible` - A boolean indicating whether the overlay should be visible (`true`) or hidden (`false`).
    pub fn set_overlay_visibility(&mut self, identifier: Option<&str>, is_visible: bool) {
        match self.get_instance_mut(identifier) {
            Ok(overlay) => overlay.set_visibility(is_visible),
            Err(e) => warn!("[OverlayManager] set_overlay_visibility failed: {}", e),
        }
    }

    /// Registers a Dart port for a specific overlay instance.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    pub fn register_dart_port(&self, identifier: Option<&str>, port: i64) {
        match self.get_instance(identifier) {
            Ok(overlay) => overlay.register_dart_port(port),
            Err(e) => warn!("[OverlayManager] register_dart_port failed: {}", e),
        }
    }

    /// Posts a boolean message to a specific overlay instance.
    pub fn post_bool_to_overlay(
        &self,
        identifier: Option<&str>,
        value: bool,
    ) -> Result<(), FlutterEmbedderError> {
        self.get_instance(identifier)
            .and_then(|overlay| overlay.post_bool(value).map_err(|e| e.to_string()))
            .map_err(|e| {
                warn!("[OverlayManager] post_bool_to_overlay failed: {}", e);
                FlutterEmbedderError::InvalidHandle
            })
    }

    /// Posts an i64 message to a specific overlay instance.
    pub fn post_i64_to_overlay(
        &self,
        identifier: Option<&str>,
        value: i64,
    ) -> Result<(), FlutterEmbedderError> {
        self.get_instance(identifier)
            .and_then(|overlay| overlay.post_i64(value).map_err(|e| e.to_string()))
            .map_err(|e| {
                warn!("[OverlayManager] post_i64_to_overlay failed: {}", e);
                FlutterEmbedderError::InvalidHandle
            })
    }

    /// Posts an f64 message to a specific overlay instance.
    pub fn post_f64_to_overlay(
        &self,
        identifier: Option<&str>,
        value: f64,
    ) -> Result<(), FlutterEmbedderError> {
        self.get_instance(identifier)
            .and_then(|overlay| overlay.post_f64(value).map_err(|e| e.to_string()))
            .map_err(|e| {
                warn!("[OverlayManager] post_f64_to_overlay failed: {}", e);
                FlutterEmbedderError::InvalidHandle
            })
    }

    /// Posts a string message to a specific overlay instance.
    pub fn post_string_to_overlay(
        &self,
        identifier: Option<&str>,
        value: &str,
    ) -> Result<(), FlutterEmbedderError> {
        self.get_instance(identifier)
            .and_then(|overlay| overlay.post_string(value).map_err(|e| e.to_string()))
            .map_err(|e| {
                warn!("[OverlayManager] post_string_to_overlay failed: {}", e);
                FlutterEmbedderError::InvalidHandle
            })
    }

    /// Posts a byte buffer to a specific overlay instance.
    pub fn post_buffer_to_overlay(
        &self,
        identifier: Option<&str>,
        buffer: &[u8],
    ) -> Result<(), FlutterEmbedderError> {
        self.get_instance(identifier)
            .and_then(|overlay| overlay.post_buffer(buffer).map_err(|e| e.to_string()))
            .map_err(|e| {
                warn!("[OverlayManager] post_buffer_to_overlay failed: {}", e);
                FlutterEmbedderError::InvalidHandle
            })
    }

    /// Sets the screen-space position for a specific overlay.
    pub fn set_overlay_position(&mut self, identifier: Option<&str>, x: i32, y: i32) {
        match self.get_instance_mut(identifier) {
            Ok(overlay) => overlay.set_position(x, y),
            Err(e) => warn!("[OverlayManager] set_overlay_position failed: {}", e),
        }
    }

    /// Registers a custom channel handler for a specific overlay instance.
    pub fn register_channel_handler_for_instance<F>(
        &mut self,
        identifier: Option<&str>,
        channel: &str,
        handler: F,
    ) where
        F: Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
    {
        match self.get_instance_mut(identifier) {
            Ok(overlay) => overlay.register_channel_handler(channel, handler),
            Err(e) => warn!("[OverlayManager] register_channel_handler failed: {}", e),
        }
    }

    /// Brings the specified overlay to the top of the Z-order.
    pub fn bring_to_front(&mut self, identifier: Option<&str>) {
        if let Ok(id_str) = self.get_instance(identifier).map(|ov| ov.name.clone()) {
            self.overlay_order.retain(|id| id != &id_str);
            self.overlay_order.push(id_str);
        }
    }

    /// Sets keyboard focus to the specified overlay and brings it to the front.
    pub fn set_keyboard_focus(&mut self, identifier: Option<&str>) {
        if let Ok(id_str) = self.get_instance(identifier).map(|ov| ov.name.clone()) {
            self.focused_overlay_id = Some(id_str.clone());
            self.bring_to_front(Some(&id_str));
        }
    }
}

impl FlutterOverlayManagerHandle {
    /// Creates and initializes a new Flutter overlay instance and adds it to the manager.
    ///
    /// This function is the primary entry point for creating a new Flutter UI surface. It
    /// handles loading the Flutter engine, preparing rendering resources, and running the
    /// Dart isolate. If an overlay with the same `identifier` already exists, it is
    /// shut down and replaced by the new instance.
    ///
    /// # What it solves
    /// This is the foundational step to get any Flutter UI running. It abstracts away the
    /// complexities of engine startup, renderer configuration, and asset loading.
    ///
    /// # Renderer Selection
    ///
    /// This function automatically determines the best available renderer. It will first
    /// attempt to initialize a hardware-accelerated **OpenGL** renderer via ANGLE.
    ///
    /// If OpenGL initialization fails for any reason (e.g., `libEGL.dll` or `libGLESv2.dll`
    /// are not found, or a graphics driver issue occurs), it will log an error and
    /// automatically fall back to a **Software** renderer. This ensures that the overlay
    /// can be displayed even on systems without proper OpenGL support.
    ///
    /// # Arguments
    ///
    /// * `swap_chain`: A reference to the host application's `IDXGISwapChain`.
    /// * `flutter_asset_build_dir`: Path to the Flutter application's build output
    ///   directory. This can be the output of a standard `flutter build windows` command
    ///   (e.g., `build/windows/runner/Release`) or the output of a `flutter assemble`
    ///   command. An example `assemble` command is:
    ///   ```bash
    ///   flutter assemble --output=build -dTargetPlatform=windows-x64 -dBuildMode={build_mode} {build_mode}_bundle_windows-x64_assets
    ///   ```
    ///   Regardless of the method used, this directory must contain the necessary Flutter
    ///   assets (`flutter_assets`), `icudtl.dat`, the compiled Dart code, and the
    ///   `flutter_engine.dll`.
    ///   - For **OpenGL** support, this directory must also contain `libEGL.dll` and `libGLESv2.dll`.
    /// * `identifier`: A unique string that identifies this overlay instance for all
    ///   subsequent API calls (e.g., "main_menu_ui").
    /// * `dart_args`: Optional. A vector of string arguments for the Dart `main()` function.
    /// * `engine_args`: Optional. A vector of command-line switches for the Flutter Engine,
    ///   typically used in debug builds.
    ///
    /// # Returns
    ///
    /// Returns `true` if the overlay was initialized successfully using either the OpenGL
    /// or Software renderer. Returns `false` if a critical error occurred and initialization
    /// failed completely. Errors are logged internally.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// let assets_path = PathBuf::from("./flutter_build");
    /// manager.init_instance(
    ///     &my_swap_chain,
    ///     &assets_path,
    ///     "main_hud",
    ///     None, // No special Dart arguments
    ///     None, // No special engine arguments
    /// );
    /// ```
    pub fn init_instance(
        &self,
        swap_chain: &IDXGISwapChain,
        flutter_asset_build_dir: &PathBuf,
        identifier: &str,
        dart_args: Option<Vec<String>>,
        engine_args: Option<Vec<String>>,
    ) -> bool {
        let mut manager = self.manager.lock();
        manager.init(
            swap_chain,
            flutter_asset_build_dir,
            identifier,
            dart_args,
            engine_args,
        )
    }

    /// Ticks all overlays and composites them onto the screen with selective clipping.
    ///
    /// This is the main rendering entry point for the overlay system. It should be called once per frame from
    /// a valid DirectX rendering context.
    ///
    /// It ensures a correct visual layering by first drawing 3D primitives (like entity highlights)
    /// **clipped to a specific game viewport**, and then compositing the main 2D Flutter UI on top,
    /// which renders to the **full screen without being clipped**.
    ///
    /// ## Arguments
    ///
    /// * `view_projection_matrix`: The combined view and projection matrix from the host
    ///   application's camera. This is required to correctly transform the 3D primitives.
    /// * `rect`: An optional `RECT` that defines the clipping area **for 3D primitives only**.
    ///   If `Some`, drawing generated by `queue_3d_triangles` will be hardware-clipped to this
    ///   rectangle. The main Flutter UI is **not affected**. If `None`, 3D primitives will also
    ///   be drawn without clipping.
    ///
    pub fn run_flutter_tick(&self, view_projection_matrix: &XMMatrix, rect: Option<&RECT>) {
        let mut manager = self.manager.lock();
        if let Some(context) = manager.shared_d3d_context.clone() {
            let time = if manager.is_paused {
                manager.time_at_pause
            } else {
                manager.start_time.elapsed().as_secs_f32()
            };

            let mut frame_params = FrameParams {
                context: &context,
                view_projection_matrix,
                screen_width: manager.screen_width as f32,
                screen_height: manager.screen_height as f32,
                time,
            };

            for id in manager.overlay_order.clone() {
                if let Some(overlay) = manager.active_instances.get_mut(&id) {
                    if overlay.is_visible() {
                        overlay.tick(&context);
                        update_interactive_widget_hover_state(overlay);

                        {
                            let _state_guard = ScopedRenderState::new(&context, rect);
                            overlay.primitive_renderer.draw(&mut frame_params);
                        }

                        overlay.post_processor.queue_texture_render(
                            &overlay.srv,
                            &overlay.effect_config,
                            overlay.x,
                            overlay.y,
                            overlay.width,
                            overlay.height,
                        );
                        overlay.post_processor.draw(&mut frame_params);
                    }
                }
            }
        }
    }

    /// Ticks all overlays to update their texture content for the current frame.
    ///
    /// # What it solves
    /// This function drives all Flutter animations and state updates. It processes
    /// scheduled tasks and renders a new frame into each overlay's texture if needed.
    /// This should be called once per frame *before* any compositing. For advanced
    /// render pipelines, this gives you a chance to work with the updated texture
    /// before it's drawn to the screen.
    ///
    /// # Example
    /// ```rust, no_run
    /// // In your main game loop
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.tick_overlays();
    /// // ... do other game logic or rendering ...
    /// manager.composite_overlays();
    /// ```
    pub fn tick_overlays(&self) {
        let manager = self.manager.lock();
        if let Some(context) = manager.shared_d3d_context.clone() {
            for overlay in manager.active_instances.values() {
                if overlay.is_visible() {
                    overlay.tick(&context);
                }
            }
        }
    }

    /// Composites (draws) all visible overlays onto the screen in their specified Z-order.
    ///
    /// # What it solves
    /// This function handles the final drawing of the user interfaces. It should be
    /// called once per frame after `tick_overlays` and after your main 3D scene has
    /// been rendered, to ensure the UI appears on top.
    ///
    /// # Example
    /// ```rust, no_run
    /// // In your main game loop
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.tick_overlays();
    /// render_my_3d_world();
    /// manager.composite_overlays(); // Draws the UI on top of the world
    /// ```
    pub fn composite_overlays(&self, view_projection_matrix: &XMMatrix) {
        let mut manager = self.manager.lock();
        if let Some(context) = manager.shared_d3d_context.clone() {
            let time = if manager.is_paused {
                manager.time_at_pause
            } else {
                manager.start_time.elapsed().as_secs_f32()
            };

            let mut frame_params = FrameParams {
                context: &context,
                view_projection_matrix,
                screen_width: manager.screen_width as f32,
                screen_height: manager.screen_height as f32,
                time,
            };

            for id in manager.overlay_order.clone() {
                if let Some(overlay) = manager.active_instances.get_mut(&id) {
                    if overlay.is_visible() {
                        update_interactive_widget_hover_state(overlay);

                        // Draw 3D primitives
                        overlay.primitive_renderer.draw(&mut frame_params);

                        // Queue and draw the 2D Flutter UI
                        overlay.post_processor.queue_texture_render(
                            &overlay.srv,
                            &overlay.effect_config,
                            overlay.x,
                            overlay.y,
                            overlay.width,
                            overlay.height,
                        );
                        overlay.post_processor.draw(&mut frame_params);
                    }
                }
            }
        }
    }

    /// Updates the screen dimensions used by the overlays.
    /// # Example
    /// ```rust, no_run
    /// // In your WndProc, when you receive a WM_SIZE message
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.update_screen_size(new_width, new_height);
    /// ```
    pub fn update_screen_size(&self, width: u32, height: u32) {
        let mut manager = self.manager.lock();
        manager.screen_width = width;
        manager.screen_height = height;
    }

    /// Pauses all shader animations for all overlays.
    ///
    /// Freezes the `time` uniform sent to any custom shaders, effectively pausing
    /// time-based visual effects. This does not pause the Flutter UI's internal animations.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // When the game is paused:
    /// manager.pause_animations();
    /// ```
    pub fn pause_animations(&self) {
        let mut manager = self.manager.lock();
        if !manager.is_paused {
            manager.time_at_pause = manager.start_time.elapsed().as_secs_f32();
            manager.is_paused = true;
        }
    }

    /// Atomically replaces an entire group of 3D primitives for a specific overlay.
    ///
    /// This is the recommended method for pushing 3D geometry to the system. Instead of
    /// adding primitives, this function replaces all primitives for a given `group_id`
    /// with the new set of vertices. This prevents stale data from persisting and
    /// simplifies per-frame logic.
    ///
    /// # Arguments
    /// * `identifier`: The unique name of the target overlay. `None` targets the single active overlay.
    /// * `group_id`: A string slice that identifies this group of primitives (e.g., "entity_highlights", "debug_boxes").
    /// * `vertices`: A slice of `Vertex3D` points that define the geometry.
    /// * `topology`: A `PrimitiveType` enum that specifies how the vertices should be connected.
    pub fn replace_primitives_in_group(
        &self,
        identifier: Option<&str>,
        group_id: &str,
        vertices: &[Vertex3D],
        topology: PrimitiveType,
    ) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.replace_queued_primitives_in_group(group_id, vertices, topology);
        }
    }

    // --- REPLACE `clear_all_queued_primitives` ---
    /// Clears all submitted 3D primitives from all groups and all active overlays.
    ///
    /// # Example
    /// ```rust,no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.clear_all_primitives();
    /// ```
    pub fn clear_all_primitives(&self) {
        let mut manager = self.manager.lock();
        for overlay in manager.active_instances.values_mut() {
            overlay.clear_all_queued_primitives();
        }
    }

    /// Clears all submitted 3D primitives from a specific group for a specific overlay.
    ///
    /// # Arguments
    /// * `identifier`: The unique name of the target overlay. `None` targets the single active overlay.
    /// * `group_id`: The ID of the group to clear (e.g., "entity_highlights").
    ///
    /// # Example
    /// ```rust,no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // Stop showing entity highlights without affecting other debug drawings.
    /// manager.clear_primitives_in_group(None, "entity_highlights");
    /// ```
    pub fn clear_primitives_in_group(&self, identifier: Option<&str>, group_id: &str) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.clear_queued_primitives_in_group(group_id);
        }
    }

    /// Takes a snapshot of all submitted 3D primitive data, making it ready for rendering.
    ///
    /// This function is the core of the "Submit & Latch" system, which resolves rendering flickers and
    /// disappearing primitives. The problem occurs because the game's update logic (which submits new data)
    /// and the render hook (`new_present`, which draws the data) can run at different rates.
    ///
    /// By calling this function once at the beginning of a render frame, we "latch" a stable, complete
    /// copy of the data into a dedicated render buffer. This guarantees that all draw calls within the
    /// same frame use the exact same data, eliminating the race condition that causes  visual artifacts.
    ///
    /// ## Usage
    /// Call this function at the very beginning of your `new_present` render hook.
    ///
    /// ```rust
    /// // in dxgi_present_hook.rs
    /// pub(crate) extern "system" fn new_present(...) -> HRESULT {
    ///     // Latch the data at the start of the frame.
    ///     if let Some(om) = get_flutter_overlay_manager_handle() {
    ///         om.latch_all_queued_primitives();
    ///     }
    ///
    ///     // ... rest of the render hook ...
    /// }
    /// ```
    pub fn latch_all_queued_primitives(&self) {
        let mut manager = self.manager.lock();
        manager.latch_all_queued_primitives();
    }

    /// Resumes all shader animations for all overlays.
    ///
    /// Unfreezes the `time` uniform sent to custom shaders, allowing visual effects
    /// to resume from where they left off.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // When the game is unpaused:
    /// manager.resume_animations();
    /// ```
    pub fn resume_animations(&self) {
        let mut manager = self.manager.lock();
        if manager.is_paused {
            manager.start_time =
                Instant::now() - std::time::Duration::from_secs_f32(manager.time_at_pause);
            manager.is_paused = false;
        }
    }

    /// Sets a post-processing effect for the **entire** overlay.
    ///
    /// Applies a full-screen shader effect to an overlay's texture, allowing for
    /// dynamic visual styles like holograms, warp fields, or color grading,
    /// controlled directly from your Rust code.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `effect` - The `PostEffect` enum variant to apply.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // Make the main menu look like a hologram
    /// manager.set_fullscreen_effect(Some("main_menu"), PostEffect::Hologram);
    /// ```
    pub fn set_fullscreen_effect(&self, identifier: Option<&str>, effect: PostEffect) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.effect_config.params = match effect {
                PostEffect::Passthrough => EffectParams::None,
                PostEffect::Hologram => EffectParams::Hologram(HologramParams::default()),
                PostEffect::WarpField => EffectParams::WarpField(WarpFieldParams::default()),
            };
            overlay.effect_config.target = EffectTarget::Fullscreen;
        }
    }

    /// Applies a post-processing effect to a **specific area** of an overlay.
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `effect` - The `PostEffect` enum variant to apply.
    /// * `bounds` - An array `[x, y, width, height]` defining the target rectangle in logical pixels.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // Make a specific button on the HUD glow with a warp effect
    /// let button_bounds = [100.0, 200.0, 150.0, 50.0];
    /// manager.set_widget_effect(Some("main_hud"), PostEffect::WarpField, button_bounds);
    /// ```
    pub fn set_widget_effect(
        &self,
        identifier: Option<&str>,
        effect: PostEffect,
        bounds: [f32; 4],
    ) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.effect_config.params = match effect {
                PostEffect::Passthrough => EffectParams::None,
                PostEffect::Hologram => EffectParams::Hologram(HologramParams::default()),
                PostEffect::WarpField => EffectParams::WarpField(WarpFieldParams::default()),
            };
            overlay.effect_config.target = EffectTarget::Widget(bounds);
        }
    }

    /// Removes any active effect from an overlay, reverting it to the default passthrough shader.
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.clear_effect(Some("main_menu"));
    /// ```
    pub fn clear_effect(&self, identifier: Option<&str>) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.effect_config = EffectConfig::default();
        }
    }

    /// Updates the entire effect configuration for a specific overlay.
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `config`: The complete `EffectConfig` struct.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// let config = EffectConfig {
    ///     target: EffectTarget::Fullscreen,
    ///     params: EffectParams::Hologram(HologramParams { intensity: 0.8, ..Default::default() }),
    /// };
    /// manager.update_effect_config(Some("main_menu"), config);
    /// ```
    pub fn update_effect_config(&self, identifier: Option<&str>, config: EffectConfig) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.effect_config = config;
        }
    }

    /// Forwards a raw Windows message to the manager for input processing.
    ///
    /// This is the critical function for making UIs interactive. It translates Windows
    /// input messages (mouse moves, clicks, key presses) into events that Flutter
    /// can understand and deliver to the appropriate widgets. Without this, your
    /// UI will be visible but completely non-interactive.
    ///
    /// # Returns
    /// `true` if a Flutter overlay consumed the event. The host application can
    /// use this to suppress further processing of the input (e.g., stop the game
    /// camera from moving when the mouse is over a UI button).
    ///
    /// # Example
    /// ```rust, no_run
    /// // Inside your application's WndProc
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// if manager.forward_input_to_flutter(hwnd, msg, wparam, lparam) {
    ///     return LRESULT(0); // Flutter handled it, so we stop processing.
    /// }
    /// // ... continue with normal message processing for the game ...
    /// ```
    pub fn forward_input_to_flutter(
        &self,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> bool {
        let mut manager = self.manager.lock();
        manager.handle_input_event(hwnd, msg, wparam, lparam)
    }

    /// Requests that the topmost active overlay under the cursor set the mouse cursor style.
    /// Call this from your `WndProc` when handling `WM_SETCURSOR`.
    ///
    /// Allows the Flutter UI to control the appearance of the mouse cursor, for example,
    /// changing it from an arrow to a text-input I-beam when hovering over a text field,
    /// or to a hand pointer over a button. This provides essential visual feedback to the user.
    ///
    /// # Returns
    /// * `Some(LRESULT(1))` if a Flutter overlay handled the cursor request.
    /// * `None` if no overlay handled the request.
    ///
    /// # Example
    /// ```rust, no_run
    /// // In your WndProc
    /// // case WM_SETCURSOR:
    /// if let Some(manager) = get_flutter_overlay_manager_handle() {
    ///     if let Some(result) = manager.set_flutter_cursor(hwnd, lparam, original_hwnd) {
    ///         return result; // Flutter handled the cursor
    ///     }
    /// }
    /// // Default handling...
    /// ```
    pub fn set_flutter_cursor(
        &self,
        hwnd_for_setcursor_message: HWND,
        lparam: LPARAM,
        original_hwnd: HWND,
    ) -> Option<LRESULT> {
        let manager = self.manager.lock();
        manager.handle_set_cursor(hwnd_for_setcursor_message, lparam, original_hwnd)
    }

    /// Notifies all active overlays of a window or render area resize.
    ///
    /// Informs all Flutter instances about the new size of the window, allowing them
    /// to recalculate layouts and adapt to the new resolution. It also ensures the
    /// underlying GPU textures are resized correctly to prevent stretching or clipping.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.resize_flutter_overlays(&my_swap_chain, 0, 0, new_width, new_height);
    /// ```
    pub fn resize_flutter_overlays(
        &self,
        swap_chain: &IDXGISwapChain,
        x_pos: i32,
        y_pos: i32,
        width: u32,
        height: u32,
    ) {
        let mut manager = self.manager.lock();
        manager.handle_resize(swap_chain, x_pos, y_pos, width, height);
    }

    /// Shuts down a specific Flutter overlay instance, releasing all its resources.
    /// # Arguments
    /// * `identifier`: The unique identifier of the overlay to shut down.
    pub fn shutdown_instance(&self, identifier: &str) {
        let mut manager = self.manager.lock();
        if let Err(e) = manager.shutdown_instance(identifier) {
            error!(
                "[OverlayManagerHandle] Error during shutdown of instance {}: {}",
                identifier, e
            );
        }
    }

    /// Shuts down all currently active Flutter overlay instances.
    /// # Example
    /// ```rust, no_run
    /// // In your application's exit/cleanup logic:
    /// if let Some(manager) = get_flutter_overlay_manager_handle() {
    ///     manager.shutdown_all_instances();
    /// }
    /// ```
    pub fn shutdown_all_instances(&self) {
        let mut manager = self.manager.lock();
        manager.shutdown_all_instances();
    }

    /// Brings the specified overlay to the top of the rendering order (Z-order).
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // When a popup is shown
    /// manager.bring_to_front(Some("popup_dialog"));
    /// ```
    pub fn bring_to_front(&self, identifier: Option<&str>) {
        let mut manager = self.manager.lock();
        manager.bring_to_front(identifier);
    }

    /// Sets keyboard focus to the specified overlay, which also brings it to the front.
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // When the user clicks on the chat input field
    /// manager.set_focus(Some("chat_ui"));
    /// ```
    pub fn set_focus(&self, identifier: Option<&str>) {
        let mut manager = self.manager.lock();
        manager.set_keyboard_focus(identifier);
    }

    /// Checks if the specified overlay currently has keyboard focus.
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// if manager.is_focused(Some("chat_ui")) {
    ///     // Don't process game movement keys
    /// }
    /// ```
    pub fn is_focused(&self, identifier: Option<&str>) -> bool {
        let manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance(identifier) {
            return manager.focused_overlay_id.as_deref() == Some(overlay.name.as_str());
        }

        false
    }

    /// Sets the visibility of a Flutter overlay. An invisible overlay is not rendered and does not receive input.
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `is_visible` - Whether the overlay should be visible.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // When the player presses the escape key:
    /// manager.set_visibility(Some("pause_menu"), true);
    /// ```
    pub fn set_visibility(&self, identifier: Option<&str>, is_visible: bool) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.set_visibility(is_visible);
        }
    }

    /// Sets the screen-space position of an overlay's top-left corner.
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `x`, `y` - The new screen-space coordinates.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // Move the health bar to follow the player
    /// manager.set_position(Some("player_health_bar"), player.x + 10, player.y - 50);
    /// ```
    pub fn set_position(&self, identifier: Option<&str>, x: i32, y: i32) {
        let mut manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance_mut(identifier) {
            overlay.set_position(x, y);
        }
    }

    /// Sends a platform message to all visible overlays.
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // Notify all UIs that the game is saving
    /// manager.broadcast_message("game/events", "saving".as_bytes());
    /// ```
    pub fn broadcast_message(&self, channel: &str, message: &[u8]) {
        let manager = self.manager.lock();
        manager.broadcast_platform_message(channel, message);
    }

    /// Registers a custom message handler for a specific channel on an overlay.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `channel` - The name of the channel the handler will listen to (e.g., "game/settings").
    /// * `handler` - A closure that processes an incoming `Vec<u8>` and returns a `Vec<u8>` response.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.register_channel_handler(Some("settings_menu"), "settings/setVolume", |payload| {
    ///     if let Some(volume_byte) = payload.get(0) {
    ///         let volume = *volume_byte as f32 / 255.0;
    ///         println!("Game volume set to {}", volume);
    ///     }
    ///     vec![1] // Return a success code as a Vec<u8>
    /// });
    /// ```
    pub fn register_channel_handler<F>(&self, identifier: Option<&str>, channel: &str, handler: F)
    where
        F: Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
    {
        let mut manager = self.manager.lock();
        manager.register_channel_handler_for_instance(identifier, channel, handler);
    }

    /// Gets the dimensions (width, height) of all active overlays.
    ///
    /// Allows the host application to get the size of all UIs, which can be useful
    /// for layout calculations, screen captures, or debugging.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// let all_sizes = manager.get_all_dimensions();
    /// for (id, (width, height)) in all_sizes {
    ///     println!("Overlay '{}' is {}x{}", id, width, height);
    /// }
    /// ```
    pub fn get_all_dimensions(&self) -> HashMap<String, (u32, u32)> {
        let manager = self.manager.lock();
        manager.get_all_overlay_dimensions()
    }

    /// Gets a clone of the shared Direct3D device context used by the manager.
    ///
    /// Provides direct access to the D3D11 context for advanced, custom rendering
    /// needs that might need to interoperate with the overlay's rendering.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// if let Some(context) = manager.get_d3d_context() {
    ///     // Perform custom D3D11 operations
    /// }
    /// ```
    pub fn get_d3d_context(&self) -> Option<ID3D11DeviceContext> {
        let manager = self.manager.lock();
        manager.shared_d3d_context.clone()
    }

    /// Finds the identifier of the topmost, visible overlay at a given screen coordinate.
    pub fn find_at_position(&self, x: i32, y: i32) -> Option<String> {
        let manager = self.manager.lock();
        manager.find_topmost_overlay_at_position(x, y)
    }

    /// Registers a Dart `SendPort` with an overlay for Rust-to-Dart communication.
    ///
    /// Establishes a direct, low-level communication channel for pushing data from Rust
    /// to Dart. This is faster than platform channels and is ideal for
    /// frequent, fire-and-forget data updates that need to be reflected in the UI every frame.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    /// * `port` - The native port handle from Dart's `ReceivePort.sendPort.nativePort`.
    ///
    /// # Example
    /// ```rust, no_run
    /// // This function would be exposed via FFI and called from Dart at startup.
    /// #[no_mangle]
    /// pub extern "C" fn register_dart_port(port: i64) {
    ///     if let Some(manager) = get_flutter_overlay_manager_handle() {
    ///         manager.register_dart_port(None, port);
    ///     }
    /// }
    /// ```
    pub fn register_dart_port(&self, identifier: Option<&str>, port: i64) {
        let manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance(identifier) {
            overlay.register_dart_port(port);
        }
    }

    /// Sends a boolean message to a single overlay via its registered `SendPort`.
    ///
    /// A fast path for pushing boolean state to Dart. See `register_dart_port` for the use case.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// manager.post_bool(Some("main_hud"), true); // e.g., show "In Combat" indicator
    /// ```
    pub fn post_bool(&self, identifier: Option<&str>, value: bool) -> bool {
        let manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance(identifier) {
            return overlay.post_bool(value).is_ok();
        }

        false
    }

    /// Sends an i64 message to a single overlay via its registered `SendPort`.
    ///
    /// A fast path for pushing integer data to Dart. See `register_dart_port` for the use case.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// let current_score = 1500;
    /// manager.post_i64(Some("main_hud"), current_score);
    /// ```
    pub fn post_i64(&self, identifier: Option<&str>, value: i64) -> bool {
        let manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance(identifier) {
            return overlay.post_i64(value).is_ok();
        }

        false
    }

    /// Sends an f64 message to a single overlay via its registered `SendPort`.
    ///
    /// A fast path for pushing floating-point data to Dart. See `register_dart_port` for the use case.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// let time_remaining = 29.5;
    /// manager.post_f64(Some("main_hud"), time_remaining);
    /// ```
    pub fn post_f64(&self, identifier: Option<&str>, value: f64) -> bool {
        let manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance(identifier) {
            return overlay.post_f64(value).is_ok();
        }

        false
    }

    /// Sends a string message to a single overlay via its registered `SendPort`.
    ///
    /// A fast path for pushing string data to Dart. See `register_dart_port` for the use case.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// // Send a quest update to the HUD
    /// manager.post_string(Some("main_hud"), "New quest: Defeat the dragon!");
    /// ```
    pub fn post_string(&self, identifier: Option<&str>, value: &str) -> bool {
        let manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance(identifier) {
            return overlay.post_string(value).is_ok();
        }

        false
    }

    /// Sends a byte buffer to a single overlay via its registered `SendPort`.
    ///
    /// A fast path for pushing raw binary data to Dart. See `register_dart_port` for the use case.
    ///
    /// # Arguments
    /// * `identifier` - The unique identifier of the overlay. If `None`, targets the single active overlay.
    ///
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// let minimap_data: Vec<u8> = vec![0, 1, 2, 3];
    /// manager.post_buffer(Some("main_hud"), &minimap_data);
    /// ```
    pub fn post_buffer(&self, identifier: Option<&str>, buffer: &[u8]) -> bool {
        let manager = self.manager.lock();
        if let Ok(overlay) = manager.get_instance(identifier) {
            return overlay.post_buffer(buffer).is_ok();
        }
        false
    }

    /// Retrieves the rendered textures from all active and visible overlays.
    /// # Example
    /// ```rust, no_run
    /// let manager = get_flutter_overlay_manager_handle().unwrap();
    /// let textures = manager.get_all_overlay_textures();
    /// for (id, texture_srv) in textures {
    ///     // Use texture_srv to draw the UI in a custom way
    /// }
    /// ```
    pub fn get_all_overlay_textures(&self) -> Vec<(String, ID3D11ShaderResourceView)> {
        let manager = self.manager.lock();
        manager.get_all_overlay_textures()
    }
}
