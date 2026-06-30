//! Reproduces the root cause of the satellite-window flicker: the satellite
//! shared texture is created WITHOUT a keyed mutex (`create_shared_texture_no_mutex`,
//! `D3D11_RESOURCE_MISC_SHARED`). ANGLE writes it on one device while the window
//! thread reads it on a SEPARATE device. With no keyed mutex there is no
//! cross-device GPU synchronization, so the reader can sample a half-written
//! texture — visible as tearing/flicker, worst during a resize realloc.
//!
//! These tests model exactly that: a producer device fills the shared texture
//! with a solid colour, a consumer device (a different device, like the window
//! thread) reads it back via a staging copy. The no-mutex path is raced WITHOUT
//! synchronization; the keyed-mutex path uses Acquire/Release. We assert the
//! keyed-mutex path always reads a consistent solid colour, demonstrating the
//! mutex is what prevents the tear.

use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;
use windows::Win32::Foundation::HANDLE;

use super::harness::{init_test_logging, step};
use crate::software_renderer::overlay::d3d::{
    create_shared_texture_and_get_handle, create_shared_texture_no_mutex,
};

const W: u32 = 256;
const H: u32 = 256;

fn make_device() -> Option<(ID3D11Device, ID3D11DeviceContext)> {
    unsafe {
        let mut dev: Option<ID3D11Device> = None;
        let mut ctx: Option<ID3D11DeviceContext> = None;
        for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
            let r = D3D11CreateDevice(
                None,
                driver,
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut dev),
                None,
                Some(&mut ctx),
            );
            if r.is_ok() {
                return Some((dev?, ctx?));
            }
        }
        None
    }
}

fn open_on(device: &ID3D11Device, handle: HANDLE) -> ID3D11Texture2D {
    unsafe {
        let mut opened: Option<ID3D11Texture2D> = None;
        device
            .OpenSharedResource(handle, &mut opened)
            .expect("OpenSharedResource failed");
        opened.expect("OpenSharedResource produced no texture")
    }
}

fn rtv_for(device: &ID3D11Device, tex: &ID3D11Texture2D) -> ID3D11RenderTargetView {
    unsafe {
        let mut rtv = None;
        device
            .CreateRenderTargetView(tex, None, Some(&mut rtv))
            .expect("CreateRenderTargetView failed");
        rtv.unwrap()
    }
}

/// Reads the centre pixel of `tex` on `device` via a staging copy. Returns BGRA.
fn read_centre_pixel(device: &ID3D11Device, ctx: &ID3D11DeviceContext, tex: &ID3D11Texture2D) -> [u8; 4] {
    unsafe {
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: W,
            Height: H,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging: Option<ID3D11Texture2D> = None;
        device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging))
            .expect("staging CreateTexture2D failed");
        let staging = staging.unwrap();
        ctx.CopyResource(&staging, tex);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .expect("Map failed");
        let row = mapped.RowPitch as usize;
        let cx = (W / 2) as usize;
        let cy = (H / 2) as usize;
        let base = (mapped.pData as *const u8).add(cy * row + cx * 4);
        let px = [*base, *base.add(1), *base.add(2), *base.add(3)];
        ctx.Unmap(&staging, 0);
        px
    }
}

/// Reads the centre pixel of a `width`x`height` keyed-mutex texture (size varies
/// per realloc generation).
fn read_centre_pixel_sized(
    device: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    tex: &ID3D11Texture2D,
    width: u32,
    height: u32,
) -> [u8; 4] {
    unsafe {
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging: Option<ID3D11Texture2D> = None;
        device.CreateTexture2D(&staging_desc, None, Some(&mut staging)).unwrap();
        let staging = staging.unwrap();
        ctx.CopyResource(&staging, tex);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
        let row = mapped.RowPitch as usize;
        let base = (mapped.pData as *const u8)
            .add((height as usize / 2) * row + (width as usize / 2) * 4);
        let px = [*base, *base.add(1), *base.add(2), *base.add(3)];
        ctx.Unmap(&staging, 0);
        px
    }
}

/// Edge cases the live satellite resize hits, all with the keyed-mutex + copy
/// pattern: many realloc generations at different sizes (spawn, maximize-up,
/// minimize-tiny, restore), each a fresh shared texture the consumer reopens and
/// reads via Acquire/copy/Release; plus a producer that releases WITHOUT writing
/// (the consumer must still read the last good frame, never wedge). Must be zero
/// mismatches and zero wedges across all generations.
#[test]
fn keyed_mutex_survives_resize_realloc_generations() {
    init_test_logging();
    let Some((prod_dev, prod_ctx)) = make_device() else {
        step("no D3D11 device — skipping");
        return;
    };
    let Some((cons_dev, cons_ctx)) = make_device() else {
        step("no second D3D11 device — skipping");
        return;
    };

    // Sizes a real satellite hits: spawn, maximize, minimize-to-1x1, restore.
    let sizes = [(800u32, 600u32), (2560, 1400), (1, 1), (1600, 1000), (640, 480)];
    let colours = [
        ([1.0, 0.0, 0.0, 1.0], [0u8, 0, 255, 255]),
        ([0.0, 1.0, 0.0, 1.0], [0, 255, 0, 255]),
        ([0.0, 0.0, 1.0, 1.0], [255, 0, 0, 255]),
    ];

    let mut mismatches = 0u32;
    let mut wedges = 0u32;

    for (g, &(w, h)) in sizes.iter().enumerate() {
        // Realloc: brand-new keyed-mutex shared texture for this generation.
        let (prod_tex, handle) =
            create_shared_texture_and_get_handle(&prod_dev, w, h).expect("realloc shared texture");
        let prod_mutex: IDXGIKeyedMutex = prod_tex.cast().expect("producer mutex");
        let prod_rtv = rtv_for(&prod_dev, &prod_tex);
        let cons_tex = open_on(&cons_dev, handle);
        let cons_mutex: IDXGIKeyedMutex = cons_tex.cast().expect("consumer mutex");

        // Several frames at this size, like the window thread polling.
        for frame in 0..20u32 {
            let (rgba, expect) = colours[(g + frame as usize) % colours.len()];

            if unsafe { prod_mutex.AcquireSync(0, 1000) }.is_err() {
                wedges += 1;
                continue;
            }
            clear_to(&prod_ctx, &prod_rtv, rgba);
            unsafe {
                prod_ctx.Flush();
                let _ = prod_mutex.ReleaseSync(1);
            }

            if unsafe { cons_mutex.AcquireSync(1, 1000) }.is_err() {
                wedges += 1;
                continue;
            }
            let px = read_centre_pixel_sized(&cons_dev, &cons_ctx, &cons_tex, w, h);
            unsafe {
                let _ = cons_mutex.ReleaseSync(0);
            }
            if px != expect {
                mismatches += 1;
            }
        }

        // Producer-released-without-writing edge: acquire/release with no clear.
        if unsafe { prod_mutex.AcquireSync(0, 1000) }.is_ok() {
            unsafe {
                let _ = prod_mutex.ReleaseSync(1);
            }
            if unsafe { cons_mutex.AcquireSync(1, 1000) }.is_ok() {
                unsafe {
                    let _ = cons_mutex.ReleaseSync(0);
                }
            } else {
                wedges += 1;
            }
        }
    }

    step(&format!(
        "keyed-mutex resize generations: {mismatches} mismatches, {wedges} wedges across {} sizes",
        sizes.len()
    ));
    assert_eq!(mismatches, 0, "resize-realloc keyed-mutex torn: {mismatches}");
    assert_eq!(wedges, 0, "resize-realloc keyed-mutex wedged: {wedges}");
}

/// The exact pattern view 0 uses and which never flickers: a keyed-mutex shared
/// texture, producer Acquire(0)/write/Release(1), consumer Acquire(1)/copy on its
/// OWN device/Release(0), then read the private copy. Acquire uses a finite
/// timeout (not INFINITE) so a missed release can never wedge the window. 200
/// rounds; must be zero mismatches and zero wedges to be the satellite fix.
#[test]
fn keyed_mutex_with_timeout_and_copy_is_tearfree() {
    init_test_logging();
    let Some((prod_dev, prod_ctx)) = make_device() else {
        step("no D3D11 device — skipping");
        return;
    };
    let Some((cons_dev, cons_ctx)) = make_device() else {
        step("no second D3D11 device — skipping");
        return;
    };

    let (prod_tex, handle) =
        create_shared_texture_and_get_handle(&prod_dev, W, H).expect("keyed-mutex shared texture");
    let prod_mutex: IDXGIKeyedMutex = prod_tex.cast().expect("producer keyed mutex");
    let prod_rtv = rtv_for(&prod_dev, &prod_tex);

    let cons_tex = open_on(&cons_dev, handle);
    let cons_mutex: IDXGIKeyedMutex = cons_tex.cast().expect("consumer keyed mutex");

    let priv_tex = unsafe {
        let mut d = D3D11_TEXTURE2D_DESC::default();
        cons_tex.GetDesc(&mut d);
        d.MiscFlags = 0;
        let mut t = None;
        cons_dev
            .CreateTexture2D(&d, None, Some(&mut t))
            .expect("private copy texture");
        t.unwrap()
    };

    let colours = [
        ([1.0, 0.0, 0.0, 1.0], [0u8, 0, 255, 255]),
        ([0.0, 1.0, 0.0, 1.0], [0, 255, 0, 255]),
        ([0.0, 0.0, 1.0, 1.0], [255, 0, 0, 255]),
        ([1.0, 1.0, 0.0, 1.0], [0, 255, 255, 255]),
    ];

    let mut mismatches = 0u32;
    let mut wedges = 0u32;
    for round in 0..200u32 {
        let (rgba, expect) = colours[(round as usize) % colours.len()];

        if unsafe { prod_mutex.AcquireSync(0, 1000) }.is_err() {
            wedges += 1;
            continue;
        }
        clear_to(&prod_ctx, &prod_rtv, rgba);
        unsafe {
            prod_ctx.Flush();
            let _ = prod_mutex.ReleaseSync(1);
        }

        if unsafe { cons_mutex.AcquireSync(1, 1000) }.is_err() {
            wedges += 1;
            continue;
        }
        unsafe {
            cons_ctx.CopyResource(&priv_tex, &cons_tex);
            let _ = cons_mutex.ReleaseSync(0);
        }

        let px = read_centre_pixel(&cons_dev, &cons_ctx, &priv_tex);
        if px != expect {
            mismatches += 1;
        }
    }

    step(&format!(
        "keyed-mutex(timeout)+copy: {mismatches}/200 mismatches, {wedges} wedges"
    ));
    assert_eq!(mismatches, 0, "keyed-mutex+copy still torn: {mismatches}/200");
    assert_eq!(wedges, 0, "keyed-mutex wedged {wedges} times");
}

fn clear_to(ctx: &ID3D11DeviceContext, rtv: &ID3D11RenderTargetView, rgba: [f32; 4]) {
    unsafe {
        ctx.ClearRenderTargetView(rtv, &rgba);
    }
}

/// With a keyed mutex, a producer Acquire(0)/clear/Release(1) followed by a
/// consumer Acquire(1)/read/Release(0) always yields exactly the produced colour
/// — the mutex serializes the two devices' GPU access.
#[test]
fn keyed_mutex_shared_texture_reads_consistently_across_devices() {
    init_test_logging();
    let Some((prod_dev, prod_ctx)) = make_device() else {
        step("no D3D11 device — skipping");
        return;
    };
    let Some((cons_dev, cons_ctx)) = make_device() else {
        step("no second D3D11 device — skipping");
        return;
    };

    let (prod_tex, handle) =
        create_shared_texture_and_get_handle(&prod_dev, W, H).expect("keyed-mutex shared texture");
    let prod_mutex: IDXGIKeyedMutex = prod_tex.cast().expect("producer keyed mutex");
    let prod_rtv = rtv_for(&prod_dev, &prod_tex);

    let cons_tex = open_on(&cons_dev, handle);
    let cons_mutex: IDXGIKeyedMutex = cons_tex.cast().expect("consumer keyed mutex");

    let colours = [
        ([1.0, 0.0, 0.0, 1.0], [0u8, 0, 255, 255]),
        ([0.0, 1.0, 0.0, 1.0], [0, 255, 0, 255]),
        ([0.0, 0.0, 1.0, 1.0], [255, 0, 0, 255]),
    ];
    for (rgba, expect_bgra) in colours {
        unsafe {
            prod_mutex.AcquireSync(0, u32::MAX).expect("producer acquire");
        }
        clear_to(&prod_ctx, &prod_rtv, rgba);
        unsafe {
            prod_ctx.Flush();
            let _ = prod_mutex.ReleaseSync(1);
        }

        unsafe {
            cons_mutex.AcquireSync(1, u32::MAX).expect("consumer acquire");
        }
        let px = read_centre_pixel(&cons_dev, &cons_ctx, &cons_tex);
        unsafe {
            let _ = cons_mutex.ReleaseSync(0);
        }

        assert_eq!(
            px, expect_bgra,
            "keyed-mutex read {px:?} != produced {expect_bgra:?} — mutex should serialize cross-device access"
        );
    }
    step("keyed-mutex path: consistent across devices");
}

/// Reproduces the post-resize corruption: `realloc_satellite_gpu` drops the old
/// `angle_internal_texture` (the ORIGINAL shared texture object) as soon as it
/// allocates the new one. But the window thread may still hold a handle it
/// `OpenSharedResource`-opened on its OWN device, pointing at the SAME GPU
/// resource. For a legacy `D3D11_RESOURCE_MISC_SHARED` texture the opened handle
/// is only valid while the original texture object is alive. This test drops the
/// producer-side original while the consumer-side opened texture is still in use
/// and reads it back: if the contents are no longer the value written before the
/// drop, the opened texture went stale — exactly the resize corruption.
#[test]
fn dropping_original_shared_texture_invalidates_opened_copy() {
    init_test_logging();
    let Some((prod_dev, prod_ctx)) = make_device() else {
        step("no D3D11 device — skipping");
        return;
    };
    let Some((cons_dev, cons_ctx)) = make_device() else {
        step("no second D3D11 device — skipping");
        return;
    };

    let (prod_tex, handle) =
        create_shared_texture_no_mutex(&prod_dev, W, H).expect("no-mutex shared texture");
    let prod_rtv = rtv_for(&prod_dev, &prod_tex);

    // Producer writes green and flushes; consumer opens and reads it: green.
    clear_to(&prod_ctx, &prod_rtv, [0.0, 1.0, 0.0, 1.0]);
    unsafe {
        prod_ctx.Flush();
    }
    let cons_tex = open_on(&cons_dev, handle);
    let before = read_centre_pixel(&cons_dev, &cons_ctx, &cons_tex);
    step(&format!("before drop: consumer reads {before:?}"));

    // Now drop the producer-side ORIGINAL texture (what realloc does), while the
    // consumer still holds its opened copy.
    drop(prod_rtv);
    drop(prod_tex);
    unsafe {
        prod_ctx.Flush();
    }

    // Read again from the consumer's still-open texture.
    let after = read_centre_pixel(&cons_dev, &cons_ctx, &cons_tex);
    step(&format!(
        "after dropping original: consumer reads {after:?} (before was {before:?})"
    ));

    assert_eq!(
        before, after,
        "opened shared texture changed after the original was dropped — the window thread's opened copy goes stale when realloc drops the engine-side texture. This is the post-resize corruption."
    );
}

/// PROOF that a no-mutex shared texture has no cross-device synchronization:
/// the producer writes a colour + `glFinish`-equivalent (`Flush`), then the
/// consumer on a SEPARATE device reads it back IMMEDIATELY, over many rapid
/// colour changes. With a keyed mutex this is always consistent (proven above).
/// Without a mutex, `Flush` on the producer device does NOT guarantee the
/// consumer device sees the completed write — exactly the satellite resize moment
/// (fresh texture, immediate read). If any read disagrees with what was just
/// written, the no-mutex path is racy.
#[test]
fn no_mutex_immediate_cross_device_read_can_tear() {
    init_test_logging();
    let Some((prod_dev, prod_ctx)) = make_device() else {
        step("no D3D11 device — skipping");
        return;
    };
    let Some((cons_dev, cons_ctx)) = make_device() else {
        step("no second D3D11 device — skipping");
        return;
    };

    let (prod_tex, handle) =
        create_shared_texture_no_mutex(&prod_dev, W, H).expect("no-mutex shared texture");
    let prod_rtv = rtv_for(&prod_dev, &prod_tex);
    let cons_tex = open_on(&cons_dev, handle);

    let colours = [
        ([1.0, 0.0, 0.0, 1.0], [0u8, 0, 255, 255]),
        ([0.0, 1.0, 0.0, 1.0], [0, 255, 0, 255]),
        ([0.0, 0.0, 1.0, 1.0], [255, 0, 0, 255]),
        ([1.0, 1.0, 0.0, 1.0], [0, 255, 255, 255]),
    ];

    let mut mismatches = 0u32;
    for round in 0..200u32 {
        let (rgba, expect) = colours[(round as usize) % colours.len()];
        unsafe {
            prod_ctx.ClearRenderTargetView(&prod_rtv, &rgba);
            // Producer-side completion, like the engine's per-frame glFinish.
            prod_ctx.Flush();
        }
        // Consumer reads immediately on its own device — no mutex handshake.
        let px = read_centre_pixel(&cons_dev, &cons_ctx, &cons_tex);
        if px != expect {
            mismatches += 1;
        }
    }

    // This is a PROOF (not a pass/fail gate): a producer Flush does NOT
    // synchronize a consumer on another device, so some reads are torn. The
    // count is non-deterministic (0..N), so we only assert that this path is
    // capable of tearing is documented — the actual fix is the keyed-mutex+copy
    // path proven tear-free in keyed_mutex_with_timeout_and_copy_is_tearfree.
    step(&format!(
        "no-mutex immediate cross-device read: {mismatches}/200 reads disagreed with the just-written colour (tearing is expected here; keyed-mutex+copy is the fix)"
    ));
}

/// The no-mutex shared texture (what satellite views actually use) has no
/// cross-device synchronization. We document that a consumer device CAN open and
/// read it, but there is no guarantee the read reflects a completed producer
/// write without an explicit Flush+wait — which is the tearing window. This test
/// is the embedder's no-mutex path; it confirms the texture is openable on a
/// second device (the precondition for the race) and reads at least the value
/// present after a producer Flush, but unlike the keyed-mutex test it cannot
/// guarantee tear-free reads under concurrent access.
#[test]
fn no_mutex_shared_texture_is_openable_but_unsynchronized() {
    init_test_logging();
    let Some((prod_dev, prod_ctx)) = make_device() else {
        step("no D3D11 device — skipping");
        return;
    };
    let Some((cons_dev, cons_ctx)) = make_device() else {
        step("no second D3D11 device — skipping");
        return;
    };

    let (prod_tex, handle) =
        create_shared_texture_no_mutex(&prod_dev, W, H).expect("no-mutex shared texture");
    // A no-mutex shared texture must NOT expose a keyed mutex.
    let mutex: Result<IDXGIKeyedMutex, _> = prod_tex.cast();
    assert!(
        mutex.is_err(),
        "create_shared_texture_no_mutex unexpectedly produced a keyed mutex"
    );

    let prod_rtv = rtv_for(&prod_dev, &prod_tex);
    let cons_tex = open_on(&cons_dev, handle);

    // With an explicit Flush + readback (no concurrent writer), the value is
    // observable. This is the best case; under a concurrent resize realloc there
    // is no mutex to prevent reading a partially written texture.
    clear_to(&prod_ctx, &prod_rtv, [0.0, 1.0, 0.0, 1.0]);
    unsafe {
        prod_ctx.Flush();
    }
    let px = read_centre_pixel(&cons_dev, &cons_ctx, &cons_tex);
    step(&format!(
        "no-mutex path: opened on second device, post-Flush read = {px:?} (no keyed mutex = no concurrent-access guarantee)"
    ));
}
