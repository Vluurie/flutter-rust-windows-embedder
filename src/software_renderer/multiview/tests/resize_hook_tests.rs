//! The satellite window resizes its swapchain via `IDXGISwapChain::ResizeBuffers`
//! (recreating the swapchain deadlocks Flutter). This test installs a vtable hook
//! on the `ResizeBuffers` slot and confirms a satellite resize calls it and the
//! view converges. The host application must keep its own ResizeBuffers hook
//! scoped to its own swapchain so satellite resizes are not misinterpreted.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use windows::Win32::Graphics::Dxgi::IDXGISwapChain;
use windows::Win32::System::Memory::{
    PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect,
};
use windows::core::{HRESULT, Interface};

use super::harness::{
    client_size, init_test_logging, resize_window_client_area, step, window_hwnd,
    with_shared_engine,
};

/// Index of `ResizeBuffers` in the `IDXGISwapChain` vtable. IUnknown (3) +
/// IDXGIObject (4) + IDXGIDeviceSubObject (1) + IDXGISwapChain methods, where
/// `ResizeBuffers` is the 13th slot (0-based).
const RESIZE_BUFFERS_VTABLE_INDEX: usize = 13;

static RESIZE_BUFFERS_HIT_COUNT: AtomicU32 = AtomicU32::new(0);
static ORIGINAL_RESIZE_BUFFERS: AtomicUsize = AtomicUsize::new(0);

type ResizeBuffersFn = unsafe extern "system" fn(
    this: *mut std::ffi::c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    new_format: DXGI_FORMAT,
    swap_chain_flags: u32,
) -> HRESULT;

/// Detour that records the call and forwards to the original. This is what the
/// game's hook conceptually is: it sees ResizeBuffers on ANY swapchain.
unsafe extern "system" fn resize_buffers_detour(
    this: *mut std::ffi::c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    new_format: DXGI_FORMAT,
    swap_chain_flags: u32,
) -> HRESULT {
    RESIZE_BUFFERS_HIT_COUNT.fetch_add(1, Ordering::SeqCst);
    let original: ResizeBuffersFn =
        unsafe { std::mem::transmute(ORIGINAL_RESIZE_BUFFERS.load(Ordering::SeqCst)) };
    unsafe { original(this, buffer_count, width, height, new_format, swap_chain_flags) }
}

/// Overwrites the `ResizeBuffers` slot in the swapchain's vtable with the detour,
/// saving the original. Because the vtable is shared by all swapchains of this
/// DXGI implementation, this is a process-global hook — exactly like the game's.
/// Returns the vtable slot address so it can be restored.
unsafe fn install_resize_buffers_hook(swap_chain: &IDXGISwapChain) -> *mut usize {
    // A COM object is a pointer to its vtable pointer.
    let obj_ptr = swap_chain.as_raw() as *const *mut usize;
    let vtable = unsafe { *obj_ptr };
    let slot = unsafe { vtable.add(RESIZE_BUFFERS_VTABLE_INDEX) };

    ORIGINAL_RESIZE_BUFFERS.store(unsafe { *slot }, Ordering::SeqCst);

    let mut old_protect = PAGE_PROTECTION_FLAGS(0);
    unsafe {
        VirtualProtect(
            slot as *const std::ffi::c_void,
            std::mem::size_of::<usize>(),
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        )
        .expect("VirtualProtect RWX on vtable slot failed");
        *slot = resize_buffers_detour as usize;
        let mut tmp = PAGE_PROTECTION_FLAGS(0);
        let _ = VirtualProtect(
            slot as *const std::ffi::c_void,
            std::mem::size_of::<usize>(),
            old_protect,
            &mut tmp,
        );
    }
    slot
}

/// Restores the original `ResizeBuffers` slot.
unsafe fn uninstall_resize_buffers_hook(slot: *mut usize) {
    let original = ORIGINAL_RESIZE_BUFFERS.load(Ordering::SeqCst);
    let mut old_protect = PAGE_PROTECTION_FLAGS(0);
    unsafe {
        if VirtualProtect(
            slot as *const std::ffi::c_void,
            std::mem::size_of::<usize>(),
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        )
        .is_ok()
        {
            *slot = original;
            let mut tmp = PAGE_PROTECTION_FLAGS(0);
            let _ = VirtualProtect(
                slot as *const std::ffi::c_void,
                std::mem::size_of::<usize>(),
                old_protect,
                &mut tmp,
            );
        }
    }
}

#[test]
fn satellite_resize_uses_resize_buffers() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        RESIZE_BUFFERS_HIT_COUNT.store(0, Ordering::SeqCst);

        let window = h.spawn("hook-test", 800, 600);
        let view_id = h.wait_for_view_id(&window, Duration::from_secs(8));
        assert!(view_id > 0, "no view id");
        assert!(
            h.wait_for_texture_size(view_id, (800, 600), Duration::from_secs(8)),
            "satellite never reached spawn size"
        );

        let slot = unsafe { install_resize_buffers_hook(&h.host_swapchain()) };
        step("installed ResizeBuffers vtable hook");

        let sat_hwnd = window_hwnd(&window);
        resize_window_client_area(sat_hwnd, 1600, 1000);
        let target = client_size(sat_hwnd);
        let converged = h.wait_for_texture_size(view_id, target, Duration::from_secs(10));

        let hits = RESIZE_BUFFERS_HIT_COUNT.load(Ordering::SeqCst);
        step(&format!(
            "satellite resize converged={converged}, ResizeBuffers hits={hits}"
        ));

        unsafe { uninstall_resize_buffers_hook(slot) };
        h.close_window(window);

        assert!(converged, "satellite did not converge to {target:?}");
        assert!(
            hits >= 1,
            "satellite resize is expected to call ResizeBuffers on its swapchain \
             (got {hits} hits)"
        );
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}
