//! Reproduces the in-game crash where maximizing a satellite window panicked the
//! task runner with `runaway expired-task batch ... — task scheduling bug`
//! (spawn.rs). Maximizing drives a large resize, which makes the engine post a
//! burst of tasks that all come due at once. The task runner must drain them
//! without panicking, and the satellite must converge to the maximized size.

use std::time::Duration;

use super::harness::{client_size, init_test_logging, step, window_hwnd, with_shared_engine};

#[test]
fn maximizing_satellite_does_not_overflow_task_runner() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        let window = h.spawn("maximize-me", 800, 600);
        let view_id = h.wait_for_view_id(&window, Duration::from_secs(8));
        assert!(view_id > 0, "no view id");
        assert!(
            h.wait_for_texture_size(view_id, (800, 600), Duration::from_secs(8)),
            "satellite never reached spawn size"
        );

        // Maximize via the same WM_APP_MAXIMIZE the Dart title-bar button posts
        // through the window-control channel.
        window.maximize();
        step("maximize requested");
        h.pump(Duration::from_secs(1));

        let target = client_size(window_hwnd(&window));
        step(&format!("maximized client size={}x{}", target.0, target.1));
        assert!(
            target.0 > 800 || target.1 > 600,
            "window did not actually maximize: client size {target:?}"
        );

        let converged = h.wait_for_texture_size(view_id, target, Duration::from_secs(10));
        step(&format!("converged to maximized size={converged}"));

        h.close_window(window);
        assert!(
            converged,
            "satellite did not converge to maximized size {target:?}"
        );
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}
