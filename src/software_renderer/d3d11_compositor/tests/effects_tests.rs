use crate::software_renderer::d3d11_compositor::effects::{HologramParams, WarpFieldParams};

#[test]
fn hologram_defaults() {
    let h = HologramParams::default();
    assert_eq!(h.aberration_amount, 0.005);
    assert_eq!(h.glitch_speed, 10.0);
    assert_eq!(h.scanline_intensity, 0.1);
}

#[test]
fn warpfield_defaults_in_range() {
    let w = WarpFieldParams::default();
    assert_eq!(w.speed, 1.0);
    assert_eq!(w.density, 2.0);
    assert_eq!(w.bloom_threshold, 0.5);
    for c in w.color_inner {
        assert!((0.0..=1.0).contains(&c));
    }
    for c in w.color_outer {
        assert!((0.0..=1.0).contains(&c));
    }
    assert!(w.base_alpha > 0.0 && w.base_alpha <= 1.0);
}
