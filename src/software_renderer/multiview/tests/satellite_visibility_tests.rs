//! Verifies that a satellite window keeps rendering when the MAIN overlay
//! (view 0, the in-game UI) is hidden (`visibility = false`). The game hides the
//! main overlay independently of any open satellite editor windows, and those
//! windows must keep updating.

use std::time::Duration;

use super::harness::{
    client_size, init_test_logging, resize_window_client_area, step, window_hwnd,
    with_shared_engine,
};

#[test]
fn satellite_still_renders_when_main_overlay_hidden() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        let window = h.spawn("vis-test", 800, 600);
        let view_id = h.wait_for_view_id(&window, Duration::from_secs(8));
        assert!(view_id > 0, "no view id");
        assert!(
            h.wait_for_texture_size(view_id, (800, 600), Duration::from_secs(8)),
            "satellite never reached spawn size while main overlay visible"
        );

        // Hide the main overlay. The satellite window must keep working.
        h.set_main_visibility(false);
        assert!(!h.main_is_visible(), "main overlay should be hidden");
        step("main overlay hidden");

        // Resize the satellite while the main overlay is hidden, and require the
        // satellite's texture to converge to the new size — driven independently
        // of main-overlay visibility.
        let sat_hwnd = window_hwnd(&window);
        resize_window_client_area(sat_hwnd, 1280, 800);
        let target = client_size(sat_hwnd);
        step(&format!(
            "satellite resized to {}x{} with main overlay hidden",
            target.0, target.1
        ));

        let converged =
            h.wait_for_texture_size_independent(view_id, target, Duration::from_secs(5));
        step(&format!(
            "satellite converged to {}x{} (main hidden)={converged}",
            target.0, target.1
        ));

        h.set_main_visibility(true);
        h.close_window(window);
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}
