use crate::software_renderer::d3d11_compositor::primitive_presets::{
    add_line, add_triangle, create_orthonormal_basis, generate_box, generate_circle_points,
    generate_sphere,
};
use crate::software_renderer::d3d11_compositor::primitive_3d_renderer::Vertex3D;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

#[test]
fn add_line_pushes_two_vertices() {
    let mut v: Vec<Vertex3D> = Vec::new();
    add_line(&mut v, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], WHITE);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].position, [0.0, 0.0, 0.0]);
    assert_eq!(v[1].position, [1.0, 0.0, 0.0]);
}

#[test]
fn add_triangle_pushes_three_vertices() {
    let mut v: Vec<Vertex3D> = Vec::new();
    add_triangle(&mut v, [0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], WHITE);
    assert_eq!(v.len(), 3);
}

#[test]
fn box_produces_12_triangles() {
    let mut v: Vec<Vertex3D> = Vec::new();
    generate_box(&mut v, [0.0; 3], [0.5, 0.5, 0.5], WHITE);
    assert_eq!(v.len(), 36);
}

#[test]
fn box_vertices_within_bounds() {
    let mut v: Vec<Vertex3D> = Vec::new();
    generate_box(&mut v, [0.0; 3], [0.5, 0.5, 0.5], WHITE);
    for vert in &v {
        for c in vert.position {
            assert!((-0.5001..=0.5001).contains(&c), "coord {c} out of bounds");
        }
    }
}

#[test]
fn box_offset_center_shifts_all_vertices() {
    let mut v: Vec<Vertex3D> = Vec::new();
    generate_box(&mut v, [10.0, 20.0, 30.0], [0.5, 0.5, 0.5], WHITE);
    for vert in &v {
        assert!((9.4999..=10.5001).contains(&vert.position[0]));
        assert!((19.4999..=20.5001).contains(&vert.position[1]));
        assert!((29.4999..=30.5001).contains(&vert.position[2]));
    }
}

#[test]
fn sphere_vertices_on_surface() {
    let mut v: Vec<Vertex3D> = Vec::new();
    let radius = 2.0;
    generate_sphere(&mut v, [0.0; 3], radius, 8, 4, WHITE);
    assert!(!v.is_empty());
    for vert in &v {
        let d = len(vert.position);
        assert!(approx(d, radius), "vertex distance {d} != radius {radius}");
    }
}

#[test]
fn orthonormal_basis_is_perpendicular_and_unit() {
    let dir = [0.0, 0.0, 1.0];
    let (t, b) = create_orthonormal_basis(dir);
    assert!(approx(len(t), 1.0), "tangent not unit: {}", len(t));
    assert!(approx(dot(dir, t), 0.0), "tangent not perpendicular");
    assert!(approx(dot(dir, b), 0.0), "bitangent not perpendicular");
}

#[test]
fn circle_points_count_and_radius() {
    let pts = generate_circle_points([0.0; 3], 3.0, [0.0, 0.0, 1.0], 12);
    assert_eq!(pts.len(), 12);
    for p in &pts {
        assert!(approx(len(*p), 3.0), "point distance {} != radius", len(*p));
    }
}
