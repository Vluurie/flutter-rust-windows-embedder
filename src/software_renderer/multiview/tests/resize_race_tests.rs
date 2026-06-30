//! Proves the post-resize flicker race: `realloc_satellite_gpu` sets the view's
//! reported `texture_size` to the new dimensions immediately after allocating the
//! new shared texture, but BEFORE the engine has rendered+presented a frame into
//! it. A window thread that treats "texture_size == window size" as "renderable"
//! will open and blit the freshly-allocated (uninitialised) texture, showing
//! garbage/flicker until the first real frame lands.
//!
//! The test catches the exact frame where `texture_size` first reports the new
//! size and checks whether `frame_counter` has advanced past the value captured
//! right after the resize. If the size flips to "new" while the counter is still
//! at its pre-render value, the renderable signal raced ahead of the content.

use std::time::{Duration, Instant};

use super::harness::{client_size, init_test_logging, resize_window_client_area, step, window_hwnd, with_shared_engine};

#[test]
fn texture_size_does_not_outrun_first_rendered_frame_after_resize() {
    init_test_logging();
    let ran = with_shared_engine(|h| {
        let window = h.spawn("resize-race", 800, 600);
        let view_id = h.wait_for_view_id(&window, Duration::from_secs(8));
        assert!(view_id > 0, "no view id");
        assert!(
            h.wait_for_texture_size(view_id, (800, 600), Duration::from_secs(8)),
            "never reached spawn size"
        );

        let hwnd = window_hwnd(&window);
        resize_window_client_area(hwnd, 1280, 800);
        let target = client_size(hwnd);
        step(&format!("resized to {}x{}", target.0, target.1));

        // Counter at the moment the resize was requested. A correct embedder
        // must present at least one new frame for the view before reporting the
        // new texture_size as the renderable size.
        let counter_before = h.view_frame_counter(view_id);

        // Drive frames and watch for the first moment texture_size becomes the
        // new size. Capture the frame counter at that exact moment.
        let start = Instant::now();
        let mut raced = false;
        let mut converged = false;
        while start.elapsed() < Duration::from_secs(8) {
            h.tick();
            let _ = h.request_frame();
            if h.view_texture_size(view_id) == Some(target) {
                let counter_now = h.view_frame_counter(view_id);
                converged = true;
                if counter_now <= counter_before {
                    raced = true;
                    step(&format!(
                        "RACE: texture_size flipped to {}x{} but frame_counter still {counter_now} (was {counter_before}) — renderable signal outran rendered content",
                        target.0, target.1
                    ));
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(4));
        }

        step(&format!("converged={converged} raced={raced}"));
        h.close_window(window);

        assert!(converged, "texture never reached {target:?} after resize");
        assert!(
            !raced,
            "post-resize flicker race: the view reported its new texture_size as renderable before the engine presented a frame into the reallocated texture"
        );
    });
    if ran.is_none() {
        step("engine unavailable — skipped");
    }
}
