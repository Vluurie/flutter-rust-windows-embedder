//! Stress tests over the shared real-engine harness. They exercise the
//! concurrency surfaces of multi-view rendering: many satellite windows on one
//! overlay (all sharing one ANGLE device), spawn/close churn (view-registry
//! add/remove + thread join), and rapid resizes (the engine resize path under
//! load). Each satellite window runs its own OS message + render thread; the
//! harness `tick()` drives the single engine frame loop they all feed, exactly
//! like the game. All tests share one long-lived overlay (the ANGLE renderer is a
//! per-process singleton), matching the game's one-overlay-many-windows model.

use std::time::Duration;

use super::harness::{
    client_size, init_test_logging, resize_window_client_area, step, window_hwnd,
    with_shared_engine,
};

/// Spawn several satellite windows at once and confirm every view reaches its
/// spawn size. Stresses concurrent window threads creating shared textures on the
/// one shared ANGLE device and racing through AddView.
#[test]
fn many_windows_all_reach_spawn_size() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        const N: usize = 6;
        let sizes = [
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
        ];

        let mut windows = Vec::new();
        for (i, &(w, ht)) in sizes.iter().take(N).enumerate() {
            let window = h.spawn(&format!("many-{i}"), w, ht);
            windows.push((window, (w, ht)));
        }
        step(&format!("spawned {N} windows"));

        let mut views = Vec::new();
        for (window, size) in &windows {
            let id = h.wait_for_view_id(window, Duration::from_secs(8));
            assert!(id > 0, "a window never published a view id");
            views.push((id, *size));
        }

        for &(id, size) in &views {
            let ok = h.wait_for_texture_size(id, size, Duration::from_secs(8));
            assert!(ok, "view {id} never reached spawn size {size:?}");
        }
        step("all views reached spawn size");

        for (window, _) in windows {
            h.close_window(window);
        }
        step("all windows closed cleanly");
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}

/// Rapidly spawn and close satellite windows in sequence. Stresses the view
/// registry add/remove path and the window-thread spawn/join lifecycle for leaks
/// or use-after-free. View ids must keep increasing (no reuse-before-free).
#[test]
fn spawn_close_churn_is_stable() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        const ROUNDS: usize = 8;
        let mut last_id = 0i64;
        for round in 0..ROUNDS {
            let window = h.spawn(&format!("churn-{round}"), 800, 600);
            let id = h.wait_for_view_id(&window, Duration::from_secs(8));
            assert!(id > 0, "round {round}: no view id");
            assert!(
                id > last_id,
                "round {round}: view id {id} not greater than previous {last_id} (reuse before free)"
            );
            last_id = id;

            let ok = h.wait_for_texture_size(id, (800, 600), Duration::from_secs(8));
            assert!(ok, "round {round}: view {id} never reached 800x600");

            h.close_window(window);
            h.pump(Duration::from_millis(64));
        }
        step(&format!(
            "{ROUNDS} spawn/close rounds stable; last view id {last_id}"
        ));
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}

/// Hammer one window with many resizes and confirm it converges to the final
/// client size. Stresses the resize path (metrics + backing-store realloc +
/// swapchain ResizeBuffers) under rapid back-to-back changes.
#[test]
fn rapid_resize_converges_to_final_size() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        let window = h.spawn("rapid-resize", 800, 600);
        let id = h.wait_for_view_id(&window, Duration::from_secs(8));
        assert!(id > 0, "no view id");
        assert!(
            h.wait_for_texture_size(id, (800, 600), Duration::from_secs(8)),
            "never reached spawn size"
        );

        let hwnd = window_hwnd(&window);
        let steps = [
            (1000, 700),
            (1280, 800),
            (900, 1100),
            (1600, 900),
            (1100, 1300),
            (1440, 1000),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (960, 540),
            (720, 1280),
        ];
        for &(w, ht) in &steps {
            resize_window_client_area(hwnd, w, ht);
            h.pump(Duration::from_millis(48));
        }

        resize_window_client_area(hwnd, 1366, 768);
        let target = client_size(hwnd);
        step(&format!(
            "final target client size={}x{}",
            target.0, target.1
        ));
        let converged = h.wait_for_texture_size(id, target, Duration::from_secs(8));
        h.close_window(window);
        assert!(
            converged,
            "rapid resize did not converge to final size {target:?}"
        );
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}

/// Spawn several windows, then resize them all and require each to converge to
/// its own final client size independently. Stresses concurrent resize paths
/// across views that share the engine + ANGLE device.
#[test]
fn concurrent_resize_across_windows() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        const N: usize = 4;
        let targets = [(1280, 720), (1024, 900), (1500, 800), (960, 1200)];

        let mut windows = Vec::new();
        for i in 0..N {
            let window = h.spawn(&format!("cr-{i}"), 800, 600);
            let id = h.wait_for_view_id(&window, Duration::from_secs(8));
            assert!(id > 0, "window {i}: no view id");
            assert!(
                h.wait_for_texture_size(id, (800, 600), Duration::from_secs(8)),
                "window {i}: never reached spawn size"
            );
            windows.push((window, id));
        }
        step(&format!("{N} windows up at spawn size"));

        let mut expected = Vec::new();
        for (i, (window, id)) in windows.iter().enumerate() {
            let hwnd = window_hwnd(window);
            let (w, ht) = targets[i];
            resize_window_client_area(hwnd, w, ht);
            expected.push((*id, client_size(hwnd)));
        }

        for (id, target) in &expected {
            let ok = h.wait_for_texture_size(*id, *target, Duration::from_secs(10));
            assert!(ok, "view {id} did not converge to {target:?}");
        }
        step("all views converged to their resized sizes");

        for (window, _) in windows {
            h.close_window(window);
        }
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}
