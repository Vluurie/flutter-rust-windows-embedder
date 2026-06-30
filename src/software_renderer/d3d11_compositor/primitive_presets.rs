use super::primitive_3d_renderer::Vertex3D;

/// Adds a single vertex to the vertex buffer.
#[inline]
pub fn add_vertex(vertices: &mut Vec<Vertex3D>, position: [f32; 3], color: [f32; 4]) {
    vertices.push(Vertex3D { position, color });
}

/// Adds a line (2 vertices) to the vertex buffer.
#[inline]
pub fn add_line(vertices: &mut Vec<Vertex3D>, start: [f32; 3], end: [f32; 3], color: [f32; 4]) {
    vertices.push(Vertex3D {
        position: start,
        color,
    });
    vertices.push(Vertex3D {
        position: end,
        color,
    });
}

/// Adds a triangle (3 vertices) to the vertex buffer.
#[inline]
pub fn add_triangle(
    vertices: &mut Vec<Vertex3D>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    color: [f32; 4],
) {
    vertices.push(Vertex3D { position: a, color });
    vertices.push(Vertex3D { position: b, color });
    vertices.push(Vertex3D { position: c, color });
}

/// Unit box corners centered at origin with half-extent of 0.5 (total size 1x1x1).
pub const UNIT_BOX_CORNERS: [[f32; 3]; 8] = [
    [-0.5, -0.5, -0.5], // 0: back-bottom-left
    [0.5, -0.5, -0.5],  // 1: back-bottom-right
    [0.5, 0.5, -0.5],   // 2: back-top-right
    [-0.5, 0.5, -0.5],  // 3: back-top-left
    [-0.5, -0.5, 0.5],  // 4: front-bottom-left
    [0.5, -0.5, 0.5],   // 5: front-bottom-right
    [0.5, 0.5, 0.5],    // 6: front-top-right
    [-0.5, 0.5, 0.5],   // 7: front-top-left
];

/// Triangle indices for a solid box (36 indices = 12 triangles = 6 faces).
pub const BOX_TRIANGLE_INDICES: [usize; 36] = [
    0, 1, 2, 0, 2, 3, // back face
    1, 5, 6, 1, 6, 2, // right face
    5, 4, 7, 5, 7, 6, // front face
    4, 0, 3, 4, 3, 7, // left face
    3, 2, 6, 3, 6, 7, // top face
    4, 5, 1, 4, 1, 0, // bottom face
];

/// Line indices for a wireframe box (24 indices = 12 edges).
pub const BOX_LINE_INDICES: [usize; 24] = [
    0, 1, 1, 2, 2, 3, 3, 0, // back face edges
    4, 5, 5, 6, 6, 7, 7, 4, // front face edges
    0, 4, 1, 5, 2, 6, 3, 7, // connecting edges
];

/// Generates a solid box with the given center, half-extents, and color.
/// The box is axis-aligned (no rotation).
pub fn generate_box(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    half_extents: [f32; 3],
    color: [f32; 4],
) {
    let corners: [[f32; 3]; 8] = UNIT_BOX_CORNERS.map(|c| {
        [
            center[0] + c[0] * half_extents[0] * 2.0,
            center[1] + c[1] * half_extents[1] * 2.0,
            center[2] + c[2] * half_extents[2] * 2.0,
        ]
    });

    for tri in BOX_TRIANGLE_INDICES.chunks_exact(3) {
        add_triangle(
            vertices,
            corners[tri[0]],
            corners[tri[1]],
            corners[tri[2]],
            color,
        );
    }
}

/// Generates a wireframe box with the given center, half-extents, and color.
pub fn generate_box_wireframe(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    half_extents: [f32; 3],
    color: [f32; 4],
) {
    let corners: [[f32; 3]; 8] = UNIT_BOX_CORNERS.map(|c| {
        [
            center[0] + c[0] * half_extents[0] * 2.0,
            center[1] + c[1] * half_extents[1] * 2.0,
            center[2] + c[2] * half_extents[2] * 2.0,
        ]
    });

    for edge in BOX_LINE_INDICES.chunks_exact(2) {
        add_line(vertices, corners[edge[0]], corners[edge[1]], color);
    }
}

/// Generates a solid sphere with the given center, radius, and color.
/// Uses latitude/longitude tessellation.
///
/// # Arguments
/// * `segments` - Number of horizontal divisions (around the equator). Higher = smoother.
/// * `rings` - Number of vertical divisions (pole to pole). Higher = smoother.
pub fn generate_sphere(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    segments: u32,
    rings: u32,
    color: [f32; 4],
) {
    let pi = std::f32::consts::PI;

    for i in 0..rings {
        let lat0 = pi * (-0.5 + (i as f32) / rings as f32);
        let z0 = radius * lat0.sin();
        let zr0 = radius * lat0.cos();
        let lat1 = pi * (-0.5 + (i as f32 + 1.0) / rings as f32);
        let z1 = radius * lat1.sin();
        let zr1 = radius * lat1.cos();

        for j in 0..segments {
            let lng0 = 2.0 * pi * (j as f32) / segments as f32;
            let lng1 = 2.0 * pi * (j as f32 + 1.0) / segments as f32;

            let p0 = [
                center[0] + zr0 * lng0.cos(),
                center[1] + z0,
                center[2] + zr0 * lng0.sin(),
            ];
            let p1 = [
                center[0] + zr0 * lng1.cos(),
                center[1] + z0,
                center[2] + zr0 * lng1.sin(),
            ];
            let p2 = [
                center[0] + zr1 * lng0.cos(),
                center[1] + z1,
                center[2] + zr1 * lng0.sin(),
            ];
            let p3 = [
                center[0] + zr1 * lng1.cos(),
                center[1] + z1,
                center[2] + zr1 * lng1.sin(),
            ];

            // Two triangles per quad
            add_triangle(vertices, p0, p1, p2, color);
            add_triangle(vertices, p2, p1, p3, color);
        }
    }
}

/// Generates a wireframe sphere with 3 orthogonal great circles.
pub fn generate_sphere_wireframe(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    segments: u32,
    color: [f32; 4],
) {
    let pi = std::f32::consts::PI;

    // Generate 3 orthogonal circles (XY, XZ, YZ planes)
    for plane in 0..3 {
        for i in 0..segments {
            let angle0 = 2.0 * pi * (i as f32) / segments as f32;
            let angle1 = 2.0 * pi * ((i + 1) as f32) / segments as f32;

            let (p0, p1) = match plane {
                0 => (
                    // XY plane
                    [
                        center[0] + radius * angle0.cos(),
                        center[1] + radius * angle0.sin(),
                        center[2],
                    ],
                    [
                        center[0] + radius * angle1.cos(),
                        center[1] + radius * angle1.sin(),
                        center[2],
                    ],
                ),
                1 => (
                    // XZ plane
                    [
                        center[0] + radius * angle0.cos(),
                        center[1],
                        center[2] + radius * angle0.sin(),
                    ],
                    [
                        center[0] + radius * angle1.cos(),
                        center[1],
                        center[2] + radius * angle1.sin(),
                    ],
                ),
                _ => (
                    // YZ plane
                    [
                        center[0],
                        center[1] + radius * angle0.cos(),
                        center[2] + radius * angle0.sin(),
                    ],
                    [
                        center[0],
                        center[1] + radius * angle1.cos(),
                        center[2] + radius * angle1.sin(),
                    ],
                ),
            };

            add_line(vertices, p0, p1, color);
        }
    }
}

/// Generates a solid cylinder (walls + caps) with the given center, radius, height, and color.
/// The cylinder is oriented along the Y axis.
pub fn generate_cylinder(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    height: f32,
    segments: u32,
    color: [f32; 4],
) {
    let half_height = height * 0.5;
    let pi = std::f32::consts::PI;

    let top_center = [center[0], center[1] + half_height, center[2]];
    let bot_center = [center[0], center[1] - half_height, center[2]];

    for i in 0..segments {
        let angle0 = 2.0 * pi * (i as f32) / segments as f32;
        let angle1 = 2.0 * pi * ((i + 1) as f32) / segments as f32;

        let x0 = radius * angle0.cos();
        let z0 = radius * angle0.sin();
        let x1 = radius * angle1.cos();
        let z1 = radius * angle1.sin();

        let p0_top = [center[0] + x0, center[1] + half_height, center[2] + z0];
        let p1_top = [center[0] + x1, center[1] + half_height, center[2] + z1];
        let p0_bot = [center[0] + x0, center[1] - half_height, center[2] + z0];
        let p1_bot = [center[0] + x1, center[1] - half_height, center[2] + z1];

        // Side walls (2 triangles per segment)
        add_triangle(vertices, p0_bot, p1_bot, p1_top, color);
        add_triangle(vertices, p0_bot, p1_top, p0_top, color);

        // Top cap
        add_triangle(vertices, top_center, p0_top, p1_top, color);

        // Bottom cap
        add_triangle(vertices, bot_center, p1_bot, p0_bot, color);
    }
}

/// Generates cylinder walls only (no caps).
pub fn generate_cylinder_walls(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    height: f32,
    segments: u32,
    color: [f32; 4],
) {
    let half_height = height * 0.5;
    let pi = std::f32::consts::PI;

    for i in 0..segments {
        let angle0 = 2.0 * pi * (i as f32) / segments as f32;
        let angle1 = 2.0 * pi * ((i + 1) as f32) / segments as f32;

        let x0 = radius * angle0.cos();
        let z0 = radius * angle0.sin();
        let x1 = radius * angle1.cos();
        let z1 = radius * angle1.sin();

        let p0_top = [center[0] + x0, center[1] + half_height, center[2] + z0];
        let p1_top = [center[0] + x1, center[1] + half_height, center[2] + z1];
        let p0_bot = [center[0] + x0, center[1] - half_height, center[2] + z0];
        let p1_bot = [center[0] + x1, center[1] - half_height, center[2] + z1];

        add_triangle(vertices, p0_bot, p1_bot, p1_top, color);
        add_triangle(vertices, p0_bot, p1_top, p0_top, color);
    }
}

/// Generates a wireframe cylinder.
pub fn generate_cylinder_wireframe(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    height: f32,
    segments: u32,
    color: [f32; 4],
) {
    let half_height = height * 0.5;
    let pi = std::f32::consts::PI;

    for i in 0..segments {
        let angle0 = 2.0 * pi * (i as f32) / segments as f32;
        let angle1 = 2.0 * pi * ((i + 1) as f32) / segments as f32;

        let x0 = radius * angle0.cos();
        let z0 = radius * angle0.sin();
        let x1 = radius * angle1.cos();
        let z1 = radius * angle1.sin();

        let p0_top = [center[0] + x0, center[1] + half_height, center[2] + z0];
        let p1_top = [center[0] + x1, center[1] + half_height, center[2] + z1];
        let p0_bot = [center[0] + x0, center[1] - half_height, center[2] + z0];
        let p1_bot = [center[0] + x1, center[1] - half_height, center[2] + z1];

        // Top circle
        add_line(vertices, p0_top, p1_top, color);
        // Bottom circle
        add_line(vertices, p0_bot, p1_bot, color);
        // Vertical edges
        add_line(vertices, p0_top, p0_bot, color);
    }
}

/// Generates a solid capsule (cylinder with hemispherical caps).
/// The capsule is oriented along the Y axis.
pub fn generate_capsule(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    height: f32,
    segments: u32,
    rings: u32,
    color: [f32; 4],
) {
    let half_height = height * 0.5;

    // Cylinder walls
    generate_cylinder_walls(vertices, center, radius, height, segments, color);

    // Top hemisphere
    let top_center = [center[0], center[1] + half_height, center[2]];
    generate_hemisphere(vertices, top_center, radius, segments, rings, true, color);

    // Bottom hemisphere
    let bot_center = [center[0], center[1] - half_height, center[2]];
    generate_hemisphere(vertices, bot_center, radius, segments, rings, false, color);
}

/// Generates a hemisphere (half sphere).
/// If `top` is true, generates the upper hemisphere; otherwise, the lower hemisphere.
pub fn generate_hemisphere(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    segments: u32,
    rings: u32,
    top: bool,
    color: [f32; 4],
) {
    let pi = std::f32::consts::PI;
    let half_rings = rings / 2;

    for i in 0..half_rings {
        let (lat0, lat1) = if top {
            (
                pi * (i as f32) / rings as f32,
                pi * ((i + 1) as f32) / rings as f32,
            )
        } else {
            (
                -pi * ((i + 1) as f32) / rings as f32,
                -pi * (i as f32) / rings as f32,
            )
        };

        let y0 = radius * lat0.sin();
        let yr0 = radius * lat0.cos();
        let y1 = radius * lat1.sin();
        let yr1 = radius * lat1.cos();

        for j in 0..segments {
            let lng0 = 2.0 * pi * (j as f32) / segments as f32;
            let lng1 = 2.0 * pi * ((j + 1) as f32) / segments as f32;

            let p0 = [
                center[0] + yr0 * lng0.cos(),
                center[1] + y0,
                center[2] + yr0 * lng0.sin(),
            ];
            let p1 = [
                center[0] + yr0 * lng1.cos(),
                center[1] + y0,
                center[2] + yr0 * lng1.sin(),
            ];
            let p2 = [
                center[0] + yr1 * lng0.cos(),
                center[1] + y1,
                center[2] + yr1 * lng0.sin(),
            ];
            let p3 = [
                center[0] + yr1 * lng1.cos(),
                center[1] + y1,
                center[2] + yr1 * lng1.sin(),
            ];

            if top {
                add_triangle(vertices, p0, p2, p1, color);
                add_triangle(vertices, p1, p2, p3, color);
            } else {
                add_triangle(vertices, p0, p1, p2, color);
                add_triangle(vertices, p2, p1, p3, color);
            }
        }
    }
}

/// Generates a ring (flat disc with a hole in the center).
/// Oriented in the XZ plane (flat, facing up).
pub fn generate_ring(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    inner_radius: f32,
    outer_radius: f32,
    segments: u32,
    color: [f32; 4],
) {
    let pi = std::f32::consts::PI;

    for i in 0..segments {
        let angle0 = 2.0 * pi * (i as f32) / segments as f32;
        let angle1 = 2.0 * pi * ((i + 1) as f32) / segments as f32;

        let inner0 = [
            center[0] + inner_radius * angle0.cos(),
            center[1],
            center[2] + inner_radius * angle0.sin(),
        ];
        let inner1 = [
            center[0] + inner_radius * angle1.cos(),
            center[1],
            center[2] + inner_radius * angle1.sin(),
        ];
        let outer0 = [
            center[0] + outer_radius * angle0.cos(),
            center[1],
            center[2] + outer_radius * angle0.sin(),
        ];
        let outer1 = [
            center[0] + outer_radius * angle1.cos(),
            center[1],
            center[2] + outer_radius * angle1.sin(),
        ];

        add_triangle(vertices, inner0, outer0, outer1, color);
        add_triangle(vertices, inner0, outer1, inner1, color);
    }
}

/// Generates a wireframe ring (two concentric circles).
pub fn generate_ring_wireframe(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    inner_radius: f32,
    outer_radius: f32,
    segments: u32,
    color: [f32; 4],
) {
    let pi = std::f32::consts::PI;

    for i in 0..segments {
        let angle0 = 2.0 * pi * (i as f32) / segments as f32;
        let angle1 = 2.0 * pi * ((i + 1) as f32) / segments as f32;

        // Inner circle
        let inner0 = [
            center[0] + inner_radius * angle0.cos(),
            center[1],
            center[2] + inner_radius * angle0.sin(),
        ];
        let inner1 = [
            center[0] + inner_radius * angle1.cos(),
            center[1],
            center[2] + inner_radius * angle1.sin(),
        ];
        add_line(vertices, inner0, inner1, color);

        // Outer circle
        let outer0 = [
            center[0] + outer_radius * angle0.cos(),
            center[1],
            center[2] + outer_radius * angle0.sin(),
        ];
        let outer1 = [
            center[0] + outer_radius * angle1.cos(),
            center[1],
            center[2] + outer_radius * angle1.sin(),
        ];
        add_line(vertices, outer0, outer1, color);
    }
}

/// Generates a solid hexagon (flat, in XZ plane).
pub fn generate_hexagon(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    color: [f32; 4],
) {
    let pi = std::f32::consts::PI;

    for i in 0..6 {
        let angle0 = (i as f32) * pi / 3.0;
        let angle1 = ((i + 1) as f32) * pi / 3.0;

        let p0 = [
            center[0] + radius * angle0.cos(),
            center[1],
            center[2] + radius * angle0.sin(),
        ];
        let p1 = [
            center[0] + radius * angle1.cos(),
            center[1],
            center[2] + radius * angle1.sin(),
        ];

        add_triangle(vertices, center, p0, p1, color);
    }
}

/// Generates a wireframe hexagon.
pub fn generate_hexagon_wireframe(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    color: [f32; 4],
) {
    let pi = std::f32::consts::PI;

    for i in 0..6 {
        let angle0 = (i as f32) * pi / 3.0;
        let angle1 = ((i + 1) as f32) * pi / 3.0;

        let p0 = [
            center[0] + radius * angle0.cos(),
            center[1],
            center[2] + radius * angle0.sin(),
        ];
        let p1 = [
            center[0] + radius * angle1.cos(),
            center[1],
            center[2] + radius * angle1.sin(),
        ];

        add_line(vertices, p0, p1, color);
    }
}

/// Generates an arrow pointing in a direction.
/// Useful for visualizing vectors, directions, forces, etc.
pub fn generate_arrow(
    vertices: &mut Vec<Vertex3D>,
    start: [f32; 3],
    end: [f32; 3],
    head_size: f32,
    color: [f32; 4],
) {
    // Arrow shaft
    add_line(vertices, start, end, color);

    // Calculate direction and perpendicular vectors
    let dir = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();

    if len < 0.0001 {
        return;
    }

    let dir_norm = [dir[0] / len, dir[1] / len, dir[2] / len];

    // Find a perpendicular vector
    let up = if dir_norm[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };

    let perp1 = cross(dir_norm, up);
    let perp1_len = (perp1[0] * perp1[0] + perp1[1] * perp1[1] + perp1[2] * perp1[2]).sqrt();
    let perp1 = [
        perp1[0] / perp1_len,
        perp1[1] / perp1_len,
        perp1[2] / perp1_len,
    ];

    let perp2 = cross(dir_norm, perp1);

    // Arrow head base point
    let base = [
        end[0] - dir_norm[0] * head_size,
        end[1] - dir_norm[1] * head_size,
        end[2] - dir_norm[2] * head_size,
    ];

    // Arrow head points
    let head_radius = head_size * 0.4;
    let h1 = [
        base[0] + perp1[0] * head_radius,
        base[1] + perp1[1] * head_radius,
        base[2] + perp1[2] * head_radius,
    ];
    let h2 = [
        base[0] - perp1[0] * head_radius,
        base[1] - perp1[1] * head_radius,
        base[2] - perp1[2] * head_radius,
    ];
    let h3 = [
        base[0] + perp2[0] * head_radius,
        base[1] + perp2[1] * head_radius,
        base[2] + perp2[2] * head_radius,
    ];
    let h4 = [
        base[0] - perp2[0] * head_radius,
        base[1] - perp2[1] * head_radius,
        base[2] - perp2[2] * head_radius,
    ];

    // Arrow head lines
    add_line(vertices, end, h1, color);
    add_line(vertices, end, h2, color);
    add_line(vertices, end, h3, color);
    add_line(vertices, end, h4, color);
}

/// Cross product of two 3D vectors.
#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Creates an orthonormal basis from a direction vector.
/// Returns (tangent, bitangent) perpendicular to the input direction.
pub fn create_orthonormal_basis(dir: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let up = if dir[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };

    let tangent = cross(dir, up);
    let len = (tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2]).sqrt();
    let tangent = [tangent[0] / len, tangent[1] / len, tangent[2] / len];

    let bitangent = cross(dir, tangent);

    (tangent, bitangent)
}

/// Generates a circle of points in a plane.
/// Useful for building more complex shapes.
pub fn generate_circle_points(
    center: [f32; 3],
    radius: f32,
    normal: [f32; 3],
    segments: u32,
) -> Vec<[f32; 3]> {
    let (tangent, bitangent) = create_orthonormal_basis(normal);
    let pi = std::f32::consts::PI;

    (0..segments)
        .map(|i| {
            let angle = 2.0 * pi * (i as f32) / segments as f32;
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            [
                center[0] + radius * (tangent[0] * cos_a + bitangent[0] * sin_a),
                center[1] + radius * (tangent[1] * cos_a + bitangent[1] * sin_a),
                center[2] + radius * (tangent[2] * cos_a + bitangent[2] * sin_a),
            ]
        })
        .collect()
}

pub fn generate_octahedron(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    color: [f32; 4],
) {
    let top = [center[0], center[1] + radius, center[2]];
    let bottom = [center[0], center[1] - radius, center[2]];
    let front = [center[0], center[1], center[2] + radius];
    let back = [center[0], center[1], center[2] - radius];
    let right = [center[0] + radius, center[1], center[2]];
    let left = [center[0] - radius, center[1], center[2]];

    add_triangle(vertices, top, front, right, color);
    add_triangle(vertices, top, right, back, color);
    add_triangle(vertices, top, back, left, color);
    add_triangle(vertices, top, left, front, color);

    add_triangle(vertices, bottom, right, front, color);
    add_triangle(vertices, bottom, back, right, color);
    add_triangle(vertices, bottom, left, back, color);
    add_triangle(vertices, bottom, front, left, color);
}

pub fn generate_octahedron_wireframe(
    vertices: &mut Vec<Vertex3D>,
    center: [f32; 3],
    radius: f32,
    color: [f32; 4],
) {
    let top = [center[0], center[1] + radius, center[2]];
    let bottom = [center[0], center[1] - radius, center[2]];
    let front = [center[0], center[1], center[2] + radius];
    let back = [center[0], center[1], center[2] - radius];
    let right = [center[0] + radius, center[1], center[2]];
    let left = [center[0] - radius, center[1], center[2]];

    add_line(vertices, top, front, color);
    add_line(vertices, top, back, color);
    add_line(vertices, top, right, color);
    add_line(vertices, top, left, color);

    add_line(vertices, bottom, front, color);
    add_line(vertices, bottom, back, color);
    add_line(vertices, bottom, right, color);
    add_line(vertices, bottom, left, color);

    add_line(vertices, front, right, color);
    add_line(vertices, right, back, color);
    add_line(vertices, back, left, color);
    add_line(vertices, left, front, color);
}

pub fn generate_beam(
    vertices: &mut Vec<Vertex3D>,
    start: [f32; 3],
    end: [f32; 3],
    thickness: f32,
    color: [f32; 4],
) {
    let dir = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();

    if len < 0.0001 {
        return;
    }

    let dir_norm = [dir[0] / len, dir[1] / len, dir[2] / len];

    let up = if dir_norm[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };

    let right = cross(dir_norm, up);
    let right_len = (right[0] * right[0] + right[1] * right[1] + right[2] * right[2]).sqrt();
    let right = [
        right[0] / right_len * thickness,
        right[1] / right_len * thickness,
        right[2] / right_len * thickness,
    ];

    let up_vec = cross(right, dir_norm);
    let up_len = (up_vec[0] * up_vec[0] + up_vec[1] * up_vec[1] + up_vec[2] * up_vec[2]).sqrt();
    let up_vec = [
        up_vec[0] / up_len * thickness,
        up_vec[1] / up_len * thickness,
        up_vec[2] / up_len * thickness,
    ];

    let s0 = [
        start[0] - right[0] - up_vec[0],
        start[1] - right[1] - up_vec[1],
        start[2] - right[2] - up_vec[2],
    ];
    let s1 = [
        start[0] + right[0] - up_vec[0],
        start[1] + right[1] - up_vec[1],
        start[2] + right[2] - up_vec[2],
    ];
    let s2 = [
        start[0] + right[0] + up_vec[0],
        start[1] + right[1] + up_vec[1],
        start[2] + right[2] + up_vec[2],
    ];
    let s3 = [
        start[0] - right[0] + up_vec[0],
        start[1] - right[1] + up_vec[1],
        start[2] - right[2] + up_vec[2],
    ];

    let e0 = [
        end[0] - right[0] - up_vec[0],
        end[1] - right[1] - up_vec[1],
        end[2] - right[2] - up_vec[2],
    ];
    let e1 = [
        end[0] + right[0] - up_vec[0],
        end[1] + right[1] - up_vec[1],
        end[2] + right[2] - up_vec[2],
    ];
    let e2 = [
        end[0] + right[0] + up_vec[0],
        end[1] + right[1] + up_vec[1],
        end[2] + right[2] + up_vec[2],
    ];
    let e3 = [
        end[0] - right[0] + up_vec[0],
        end[1] - right[1] + up_vec[1],
        end[2] - right[2] + up_vec[2],
    ];

    add_triangle(vertices, s0, s1, e1, color);
    add_triangle(vertices, s0, e1, e0, color);

    add_triangle(vertices, s3, e2, s2, color);
    add_triangle(vertices, s3, e3, e2, color);

    add_triangle(vertices, s1, s2, e2, color);
    add_triangle(vertices, s1, e2, e1, color);

    add_triangle(vertices, s0, e3, s3, color);
    add_triangle(vertices, s0, e0, e3, color);

    add_triangle(vertices, s0, s3, s2, color);
    add_triangle(vertices, s0, s2, s1, color);

    add_triangle(vertices, e0, e1, e2, color);
    add_triangle(vertices, e0, e2, e3, color);
}
