use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, Once};

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
use crate::software_renderer::api::FlutterEmbedderError;
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
}

impl OverlayManager {
    /// Creates a new, empty `OverlayManager`.
    fn new() -> Self {
        OverlayManager {
            active_instances: HashMap::new(),
            overlay_order: Vec::new(),
            focused_overlay_id: None,
            shared_d3d_context: None,
        }
    }

    pub fn is_focused(&self, identifier: &str) -> bool {
        self.focused_overlay_id.as_deref() == Some(identifier)
    }

    pub fn get_all_overlay_dimensions(&self) -> HashMap<String, (u32, u32)> {
        self.active_instances
            .iter()
            .map(|(id, overlay)| (id.clone(), overlay.get_dimensions()))
            .collect()
    }

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

        let mut desc: DXGI_SWAP_CHAIN_DESC = unsafe { std::mem::zeroed() };
        if let Err(e) = unsafe { swap_chain.GetDesc(&mut desc) } {
            error!(
                "[OverlayManager:{}] Failed to get SwapChain description: {:?}",
                identifier, e
            );
            return false;
        }
        let width = desc.BufferDesc.Width;
        let height = desc.BufferDesc.Height;

        let initial_x = 0;
        let initial_y = 0;

        init_logging();

        match FlutterOverlay::create(
            identifier.to_string(),
            &device,
            swap_chain,
            initial_x,
            initial_y,
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
    fn run(&self, painter: &mut dyn FlutterPainter) {
        if self.active_instances.is_empty() {
            return;
        }

        //  overlays in Z-order (bottom to top)
        for id in &self.overlay_order {
            if let Some(overlay_instance) = self.active_instances.get(id) {
                if !overlay_instance.is_visible() {
                    continue;
                }

                if overlay_instance.engine.0.is_null() {
                    continue;
                }
                if let Err(e) = overlay_instance.request_frame() {
                    error!("[OverlayManager:{}] request_frame failed: {}", id, e);
                }
                update_interactive_widget_hover_state(&overlay_instance);
                Self::paint_flutter_overlay(overlay_instance, painter, id);
            }
        }

        if let Some(d3d_ctx) = self.shared_d3d_context.as_ref() {
            for overlay_instance in self.active_instances.values() {
                if overlay_instance.is_visible() && !overlay_instance.engine.0.is_null() {
                    overlay_instance.tick(d3d_ctx);
                }
            }
        }
    }

    /// Helper function to draw a single overlay.
    fn paint_flutter_overlay(
        overlay_instance: &FlutterOverlay,
        painter: &mut dyn FlutterPainter,
        identifier: &str,
    ) {
        match overlay_instance.get_texture_srv() {
            Ok(srv) => {
                 let (x, y) = overlay_instance.get_position();
                 let (width, height) = overlay_instance.get_dimensions();
                painter.paint_texture(&srv, x, y, width, height);
            }
            Err(e) => error!(
                "[OverlayManager:{}] Failed to get SRV for compositing: {}",
                identifier, e
            ),
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
    fn handle_resize(&mut self, swap_chain: &IDXGISwapChain, x_pos: i32, y_pos: i32,  width: u32, height: u32) {
        if self.active_instances.is_empty() {
            return;
        }
        let device = match unsafe { swap_chain.GetDevice::<ID3D11Device>() } {
            Ok(d) => d,
            Err(e) => {
                error!(
                    "[OverlayManager] Failed to get D3DDevice for resize: {:?}",
                    e
                );
                return;
            }
        };
        for (id, overlay_instance) in self.active_instances.iter_mut() {
            if !overlay_instance.engine.0.is_null() {
                overlay_instance.handle_window_resize(x_pos, y_pos, width, height, &device);
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
}

impl FlutterOverlayManagerHandle {
    /// Initializes a new Flutter overlay instance. Returns true on success.
    pub fn init_instance(
        &self,
        swap_chain: &IDXGISwapChain,
        flutter_asset_dir: &PathBuf,
        identifier: &str,
        dart_args: Option<Vec<String>>,
        engine_args: Option<Vec<String>>,
    ) -> bool {
        if let Ok(mut manager) = self.manager.lock() {
            manager.init(
                swap_chain,
                flutter_asset_dir,
                identifier,
                dart_args,
                engine_args,
            )
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for init_instance.");
            false
        }
    }

    /// Runs the tick and rendering logic for all active Flutter overlays.
    pub fn run_flutter_tick<T: FlutterPainter>(&self, painter: &mut T) {
        if let Ok(manager) = self.manager.lock() {
            manager.run(painter);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for run_flutter_tick.");
        }
    }

    /// Forwards a Windows input message to the appropriate Flutter overlay(s).
    /// Returns `true` if the message was consumed by any Flutter overlay.
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

    /// Asks active Flutter overlays to set the cursor.
    /// Returns `Some(LRESULT)` if an overlay handled the cursor, `None` otherwise.
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

    /// Notifies all Flutter overlays of a window resize.
    pub fn resize_flutter_overlays(&self, swap_chain: &IDXGISwapChain,x_pos: i32, y_pos: i32,  width: u32, height: u32) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.handle_resize(swap_chain, x_pos, y_pos, width, height);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for resize_flutter_overlays.");
        }
    }

    /// Shuts down a specific Flutter overlay instance by its identifier.
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

    /// Brings the specified overlay to the top of the Z-order.
    pub fn bring_to_front(&self, identifier: &str) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.bring_to_front(identifier);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for bring_to_front.");
        }
    }

    /// Sets keyboard focus to the specified overlay and brings it to the front.
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
    ///
    /// An overlay that is not visible will not be rendered and will not receive
    /// any mouse or keyboard input.
    pub fn set_visibility(&self, identifier: &str, is_visible: bool) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.set_overlay_visibility(identifier, is_visible);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for set_visibility.");
        }
    }

    /// Sets the screen-space position of a specific overlay.
    pub fn set_position(&self, identifier: &str, x: i32, y: i32) {
        if let Ok(mut manager) = self.manager.lock() {
            manager.set_overlay_position(identifier, x, y);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for set_position.");
        }
    }

    /// Sends a platform message to all visible overlays simultaneously.
    pub fn broadcast_message(&self, channel: &str, message: &[u8]) {
        if let Ok(manager) = self.manager.lock() {
            manager.broadcast_platform_message(channel, message);
        } else {
            error!("[OverlayManagerHandle] Failed to lock manager for broadcast_message.");
        }
    }

    /// Gets the dimensions of all overlays. Returns an empty map on failure.
    pub fn get_all_dimensions(&self) -> HashMap<String, (u32, u32)> {
        self.manager.lock().map_or(HashMap::new(), |manager| {
            manager.get_all_overlay_dimensions()
        })
    }

    /// Gets a clone of the shared Direct3D device context.
    pub fn get_d3d_context(&self) -> Option<ID3D11DeviceContext> {
        self.manager
            .lock()
            .ok()
            .and_then(|manager| manager.get_d3d_context())
    }

    /// Finds the ID of the topmost, visible overlay at a given screen coordinate.
    pub fn find_at_position(&self, x: i32, y: i32) -> Option<String> {
        self.manager
            .lock()
            .ok()
            .and_then(|manager| manager.find_topmost_overlay_at_position(x, y))
    }
}
