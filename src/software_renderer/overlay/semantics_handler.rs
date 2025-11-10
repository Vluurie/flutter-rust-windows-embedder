use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow};

use crate::bindings::embedder::{
    FlutterCheckState_kFlutterCheckStateMixed, FlutterCheckState_kFlutterCheckStateTrue,
    FlutterRect, FlutterSemanticsFlag, FlutterSemanticsFlag_kFlutterSemanticsFlagHasCheckedState,
    FlutterSemanticsFlag_kFlutterSemanticsFlagHasEnabledState,
    FlutterSemanticsFlag_kFlutterSemanticsFlagHasExpandedState,
    FlutterSemanticsFlag_kFlutterSemanticsFlagHasImplicitScrolling,
    FlutterSemanticsFlag_kFlutterSemanticsFlagHasSelectedState,
    FlutterSemanticsFlag_kFlutterSemanticsFlagHasToggledState,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsButton,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsCheckStateMixed,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsChecked,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsEnabled,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsExpanded,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsFocusable,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsFocused,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsHeader,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsHidden,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsImage,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsInMutuallyExclusiveGroup,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsKeyboardKey,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsLink,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsLiveRegion,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsMultiline,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsObscured,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsReadOnly,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsSelected,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsSlider,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsTextField,
    FlutterSemanticsFlag_kFlutterSemanticsFlagIsToggled,
    FlutterSemanticsFlag_kFlutterSemanticsFlagNamesRoute,
    FlutterSemanticsFlag_kFlutterSemanticsFlagScopesRoute, FlutterSemanticsFlags,
    FlutterSemanticsNode2, FlutterSemanticsUpdate2, FlutterTransformation,
    FlutterTristate_kFlutterTristateTrue,
};
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RustSemanticsFlag {
    HasCheckedState,
    IsChecked,
    IsSelected,
    IsButton,
    IsTextField,
    IsFocused,
    HasEnabledState,
    IsEnabled,
    IsInMutuallyExclusiveGroup,
    IsHeader,
    IsObscured,
    ScopesRoute,
    NamesRoute,
    IsHidden,
    IsImage,
    IsLiveRegion,
    HasToggledState,
    IsToggled,
    HasImplicitScrolling,
    IsMultiline,
    IsReadOnly,
    IsFocusable,
    IsLink,
    IsSlider,
    IsKeyboardKey,
    IsCheckStateMixed,
    HasExpandedState,
    IsExpanded,
    HasSelectedState,
}

pub fn ffi_flags_to_rust_set(ffi_flag_value: FlutterSemanticsFlag) -> HashSet<RustSemanticsFlag> {
    let mut set = HashSet::new();
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagHasCheckedState) != 0 {
        set.insert(RustSemanticsFlag::HasCheckedState);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsChecked) != 0 {
        set.insert(RustSemanticsFlag::IsChecked);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsSelected) != 0 {
        set.insert(RustSemanticsFlag::IsSelected);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsButton) != 0 {
        set.insert(RustSemanticsFlag::IsButton);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsTextField) != 0 {
        set.insert(RustSemanticsFlag::IsTextField);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsFocused) != 0 {
        set.insert(RustSemanticsFlag::IsFocused);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagHasEnabledState) != 0 {
        set.insert(RustSemanticsFlag::HasEnabledState);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsEnabled) != 0 {
        set.insert(RustSemanticsFlag::IsEnabled);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsInMutuallyExclusiveGroup) != 0
    {
        set.insert(RustSemanticsFlag::IsInMutuallyExclusiveGroup);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsHeader) != 0 {
        set.insert(RustSemanticsFlag::IsHeader);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsObscured) != 0 {
        set.insert(RustSemanticsFlag::IsObscured);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagScopesRoute) != 0 {
        set.insert(RustSemanticsFlag::ScopesRoute);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagNamesRoute) != 0 {
        set.insert(RustSemanticsFlag::NamesRoute);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsHidden) != 0 {
        set.insert(RustSemanticsFlag::IsHidden);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsImage) != 0 {
        set.insert(RustSemanticsFlag::IsImage);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsLiveRegion) != 0 {
        set.insert(RustSemanticsFlag::IsLiveRegion);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagHasToggledState) != 0 {
        set.insert(RustSemanticsFlag::HasToggledState);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsToggled) != 0 {
        set.insert(RustSemanticsFlag::IsToggled);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagHasImplicitScrolling) != 0 {
        set.insert(RustSemanticsFlag::HasImplicitScrolling);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsMultiline) != 0 {
        set.insert(RustSemanticsFlag::IsMultiline);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsReadOnly) != 0 {
        set.insert(RustSemanticsFlag::IsReadOnly);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsFocusable) != 0 {
        set.insert(RustSemanticsFlag::IsFocusable);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsLink) != 0 {
        set.insert(RustSemanticsFlag::IsLink);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsSlider) != 0 {
        set.insert(RustSemanticsFlag::IsSlider);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsKeyboardKey) != 0 {
        set.insert(RustSemanticsFlag::IsKeyboardKey);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsCheckStateMixed) != 0 {
        set.insert(RustSemanticsFlag::IsCheckStateMixed);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagHasExpandedState) != 0 {
        set.insert(RustSemanticsFlag::HasExpandedState);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagIsExpanded) != 0 {
        set.insert(RustSemanticsFlag::IsExpanded);
    }
    if (ffi_flag_value & FlutterSemanticsFlag_kFlutterSemanticsFlagHasSelectedState) != 0 {
        set.insert(RustSemanticsFlag::HasSelectedState);
    }
    set
}

/// Converts the new FlutterSemanticsFlags struct (Flutter 3.57+) to our internal RustSemanticsFlag set
pub fn ffi_flags2_to_rust_set(flags: &FlutterSemanticsFlags) -> HashSet<RustSemanticsFlag> {
    let mut set = HashSet::new();

    // Check state
    if flags.is_checked == FlutterCheckState_kFlutterCheckStateTrue {
        set.insert(RustSemanticsFlag::HasCheckedState);
        set.insert(RustSemanticsFlag::IsChecked);
    } else if flags.is_checked == FlutterCheckState_kFlutterCheckStateMixed {
        set.insert(RustSemanticsFlag::HasCheckedState);
        set.insert(RustSemanticsFlag::IsCheckStateMixed);
    }

    // Selected state
    if flags.is_selected == FlutterTristate_kFlutterTristateTrue {
        set.insert(RustSemanticsFlag::HasSelectedState);
        set.insert(RustSemanticsFlag::IsSelected);
    }

    // Enabled state
    if flags.is_enabled == FlutterTristate_kFlutterTristateTrue {
        set.insert(RustSemanticsFlag::HasEnabledState);
        set.insert(RustSemanticsFlag::IsEnabled);
    }

    // Toggled state
    if flags.is_toggled == FlutterTristate_kFlutterTristateTrue {
        set.insert(RustSemanticsFlag::HasToggledState);
        set.insert(RustSemanticsFlag::IsToggled);
    }

    // Expanded state
    if flags.is_expanded == FlutterTristate_kFlutterTristateTrue {
        set.insert(RustSemanticsFlag::HasExpandedState);
        set.insert(RustSemanticsFlag::IsExpanded);
    }

    // Focused
    if flags.is_focused == FlutterTristate_kFlutterTristateTrue {
        set.insert(RustSemanticsFlag::IsFocused);
        set.insert(RustSemanticsFlag::IsFocusable);
    }

    // Boolean flags
    if flags.is_button { set.insert(RustSemanticsFlag::IsButton); }
    if flags.is_text_field { set.insert(RustSemanticsFlag::IsTextField); }
    if flags.is_in_mutually_exclusive_group { set.insert(RustSemanticsFlag::IsInMutuallyExclusiveGroup); }
    if flags.is_header { set.insert(RustSemanticsFlag::IsHeader); }
    if flags.is_obscured { set.insert(RustSemanticsFlag::IsObscured); }
    if flags.scopes_route { set.insert(RustSemanticsFlag::ScopesRoute); }
    if flags.names_route { set.insert(RustSemanticsFlag::NamesRoute); }
    if flags.is_hidden { set.insert(RustSemanticsFlag::IsHidden); }
    if flags.is_image { set.insert(RustSemanticsFlag::IsImage); }
    if flags.is_live_region { set.insert(RustSemanticsFlag::IsLiveRegion); }
    if flags.has_implicit_scrolling { set.insert(RustSemanticsFlag::HasImplicitScrolling); }
    if flags.is_multiline { set.insert(RustSemanticsFlag::IsMultiline); }
    if flags.is_read_only { set.insert(RustSemanticsFlag::IsReadOnly); }
    if flags.is_link { set.insert(RustSemanticsFlag::IsLink); }
    if flags.is_slider { set.insert(RustSemanticsFlag::IsSlider); }
    if flags.is_keyboard_key { set.insert(RustSemanticsFlag::IsKeyboardKey); }

    set
}

fn cchar_to_string_safe(c_str: *const ::std::os::raw::c_char) -> String {
    if c_str.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(c_str).to_string_lossy().into_owned() }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessedSemanticsNode {
    pub id: i32,
    pub flags: HashSet<RustSemanticsFlag>,
    pub rect: FlutterRect,
    pub transform_to_parent: FlutterTransformation,
    pub children_in_hit_test_order: Vec<i32>,
    pub label: String,
}

pub extern "C" fn semantics_update_callback(
    update: *const FlutterSemanticsUpdate2,
    user_data: *mut ::std::os::raw::c_void,
) {
    unsafe {
        if update.is_null() {
            return;
        }
        if user_data.is_null() {
            return;
        }

        let update_ref = &*update;

        let overlay: &mut FlutterOverlay = &mut *(user_data as *mut FlutterOverlay);

        if update_ref.node_count == 0 {
            if let Ok(mut tree_guard) = overlay.semantics_tree_data.lock() {
                if !tree_guard.is_empty() {
                    tree_guard.clear();
                }
            }
            return;
        }

        let mut new_tree_snapshot = HashMap::new();

        for i in 0..update_ref.node_count {
            let node_ptr_ptr = update_ref.nodes.add(i as usize);

            if node_ptr_ptr.is_null() {
                continue;
            }

            let ffi_node_ptr = *node_ptr_ptr;
            if ffi_node_ptr.is_null() {
                continue;
            }
            let ffi_node: &FlutterSemanticsNode2 = &*ffi_node_ptr;

            let children =
                if ffi_node.children_in_hit_test_order.is_null() || ffi_node.child_count == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(
                        ffi_node.children_in_hit_test_order,
                        ffi_node.child_count as usize,
                    )
                    .to_vec()
                };

            let current_label = cchar_to_string_safe(ffi_node.label);
            // Use flags2 (new API) if available, otherwise fall back to deprecated flags
            let current_flags_set = if !ffi_node.flags2.is_null() {
                ffi_flags2_to_rust_set(&*ffi_node.flags2)
            } else {
                ffi_flags_to_rust_set(ffi_node.flags__deprecated__)
            };

            new_tree_snapshot.insert(
                ffi_node.id,
                ProcessedSemanticsNode {
                    id: ffi_node.id,
                    flags: current_flags_set,
                    rect: ffi_node.rect,
                    transform_to_parent: ffi_node.transform,
                    children_in_hit_test_order: children,
                    label: current_label,
                },
            );
        }

        if let Ok(mut tree_guard) = overlay.semantics_tree_data.lock() {
            *tree_guard = new_tree_snapshot;
        }
    }
}
fn is_point_in_flutter_rect(point_x: f64, point_y: f64, rect: &FlutterRect) -> bool {
    point_x >= rect.left && point_x <= rect.right && point_y >= rect.top && point_y <= rect.bottom
}

fn invert_affine_transform(t: &FlutterTransformation) -> Option<FlutterTransformation> {
    let det = t.scaleX * t.scaleY - t.skewX * t.skewY;
    if det.abs() < 1e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some(FlutterTransformation {
        scaleX: t.scaleY * inv_det,
        skewX: -t.skewX * inv_det,
        transX: (t.skewX * t.transY - t.scaleY * t.transX) * inv_det,
        skewY: -t.skewY * inv_det,
        scaleY: t.scaleX * inv_det,
        transY: (t.skewY * t.transX - t.scaleX * t.transY) * inv_det,
        pers0: 0.0,
        pers1: 0.0,
        pers2: 1.0,
    })
}

fn apply_transform_to_point(x: f64, y: f64, t: &FlutterTransformation) -> (f64, f64) {
    let new_x = t.scaleX * x + t.skewX * y + t.transX;
    let new_y = t.skewY * x + t.scaleY * y + t.transY;
    (new_x, new_y)
}

fn hit_test_node_recursive(
    node_id: i32,
    mouse_x_in_parent_cs: f64,
    mouse_y_in_parent_cs: f64,
    tree: &HashMap<i32, ProcessedSemanticsNode>,
) -> Option<i32> {
    if let Some(node) = tree.get(&node_id) {
        let (local_mouse_x, local_mouse_y) =
            if let Some(inv_transform) = invert_affine_transform(&node.transform_to_parent) {
                apply_transform_to_point(mouse_x_in_parent_cs, mouse_y_in_parent_cs, &inv_transform)
            } else {
                (mouse_x_in_parent_cs, mouse_y_in_parent_cs)
            };

        for child_id_ptr in node.children_in_hit_test_order.iter().rev() {
            if let Some(hit_child_id) =
                hit_test_node_recursive(*child_id_ptr, local_mouse_x, local_mouse_y, tree)
            {
                return Some(hit_child_id);
            }
        }

        if is_point_in_flutter_rect(local_mouse_x, local_mouse_y, &node.rect) {
            let flags = &node.flags;
            let is_interactive = flags.contains(&RustSemanticsFlag::IsButton)
                || flags.contains(&RustSemanticsFlag::IsTextField)
                || flags.contains(&RustSemanticsFlag::IsFocusable)
                || flags.contains(&RustSemanticsFlag::IsLink)
                || flags.contains(&RustSemanticsFlag::IsSlider);

            if is_interactive {
                return Some(node.id);
            }
        }
    }
    None
}

pub fn update_interactive_widget_hover_state(overlay: &FlutterOverlay) {
    let mut cursor_pos_screen: POINT = POINT { x: 0, y: 0 };

    let overlay_hwnd = overlay.windows_handler;

    unsafe {
        if GetCursorPos(&mut cursor_pos_screen).is_err() {
            overlay
                .is_interactive_widget_hovered
                .store(false, Ordering::Relaxed);
            return;
        }

        if GetForegroundWindow() != overlay_hwnd.0 {
            overlay
                .is_interactive_widget_hovered
                .store(false, Ordering::Relaxed);
            return;
        }

        let mut client_cursor_pos = cursor_pos_screen;

        if !ScreenToClient(overlay_hwnd.0, &mut client_cursor_pos).as_bool() {
            overlay
                .is_interactive_widget_hovered
                .store(false, Ordering::Relaxed);
            return;
        }

        let mouse_x_for_flutter = client_cursor_pos.x as f64;
        let mouse_y_for_flutter = client_cursor_pos.y as f64;

        let new_hover_state = {
            if let Ok(tree_guard) = overlay.semantics_tree_data.lock() {
                if !tree_guard.is_empty() {
                    hit_test_node_recursive(
                        0,
                        mouse_x_for_flutter,
                        mouse_y_for_flutter,
                        &tree_guard,
                    )
                    .is_some()
                } else {
                    false
                }
            } else {
                false
            }
        };
        overlay
            .is_interactive_widget_hovered
            .store(new_hover_state, Ordering::Relaxed);
    }
}
