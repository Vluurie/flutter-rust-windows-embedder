use crate::software_renderer::d3d11_compositor::text_presets::{
    create_fixed_width_glyph_map, transform_point,
};

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn transform_point_identity_basis() {
    let p = transform_point([1.0, 2.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 1.0);
    assert_eq!(p, [2.0, 3.0, 0.0]);
}

#[test]
fn transform_point_zero_offset() {
    let p = transform_point([5.0, 5.0, 5.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.0, 0.0);
    assert_eq!(p, [5.0, 5.0, 5.0]);
}

#[test]
fn glyph_map_covers_ascii_range() {
    let map = create_fixed_width_glyph_map(16, 8.0, 16.0, 128.0, 256.0, 32, 126);
    assert_eq!(map.len(), 95);
    assert!(map.contains_key(&' '));
    assert!(map.contains_key(&'~'));
    assert!(map.contains_key(&'A'));
}

#[test]
fn glyph_metrics_match_cell() {
    let map = create_fixed_width_glyph_map(16, 8.0, 16.0, 128.0, 256.0, 32, 126);
    let g = map[&'A'];
    assert_eq!(g.width, 8.0);
    assert_eq!(g.height, 16.0);
    assert_eq!(g.advance, 8.0);
    assert!(approx(g.uv_rect[2], 0.0625));
    assert!(approx(g.uv_rect[3], 0.0625));
}

#[test]
fn glyph_uv_grid_position() {
    let map = create_fixed_width_glyph_map(16, 8.0, 16.0, 128.0, 256.0, 32, 126);
    let space = map[&' '];
    assert!(approx(space.uv_rect[0], 0.0));
    assert!(approx(space.uv_rect[1], 0.0));
    let code = (32 + 16) as u8 as char;
    let g = map[&code];
    assert!(approx(g.uv_rect[0], 0.0));
    assert!(approx(g.uv_rect[1], 0.0625));
}
