use super::text_3d_renderer::{FontAtlas, GlyphInfo, TexturedVertex3D};

/// Generates vertices for rendering a text string in 3D space.
///
/// The text is rendered as a series of textured quads, one per character.
/// Each quad is positioned based on the font's glyph metrics.
///
/// # Arguments
/// * `text` - The text string to render
/// * `position` - The 3D position of the text origin (left baseline by default)
/// * `font_atlas` - The font atlas containing glyph information
/// * `scale` - Scale factor (1.0 = base font size)
/// * `color` - RGBA color for the text
/// * `right` - The "right" direction vector for text layout (e.g., [1, 0, 0] for +X)
/// * `up` - The "up" direction vector for text layout (e.g., [0, 1, 0] for +Y)
///
/// # Returns
/// A vector of vertices forming triangle pairs for each character.
pub fn generate_text_vertices(
    text: &str,
    position: [f32; 3],
    font_atlas: &FontAtlas,
    scale: f32,
    color: [f32; 4],
    right: [f32; 3],
    up: [f32; 3],
) -> Vec<TexturedVertex3D> {
    let mut vertices = Vec::with_capacity(text.len() * 6);
    let world_scale = scale / font_atlas.base_font_size;

    let mut cursor_x = 0.0f32;
    let mut cursor_y = 0.0f32;

    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = 0.0;
            cursor_y -= font_atlas.line_height;
            continue;
        }

        if ch == '\r' {
            continue;
        }

        let glyph = match font_atlas.glyphs.get(&ch) {
            Some(g) => g,
            None => {
                // Try to use space or skip unknown characters
                if let Some(space) = font_atlas.glyphs.get(&' ') {
                    cursor_x += space.advance;
                }
                continue;
            }
        };

        // Skip space characters (no geometry needed, just advance cursor)
        if ch == ' ' || ch == '\t' {
            let advance = if ch == '\t' { glyph.advance * 4.0 } else { glyph.advance };
            cursor_x += advance;
            continue;
        }

        // Calculate glyph quad corners in local space
        let x0 = cursor_x + glyph.bearing_x;
        let y0 = cursor_y + glyph.bearing_y - glyph.height;
        let x1 = x0 + glyph.width;
        let y1 = cursor_y + glyph.bearing_y;

        // Transform to world space
        let p00 = transform_point(position, right, up, x0 * world_scale, y0 * world_scale);
        let p10 = transform_point(position, right, up, x1 * world_scale, y0 * world_scale);
        let p01 = transform_point(position, right, up, x0 * world_scale, y1 * world_scale);
        let p11 = transform_point(position, right, up, x1 * world_scale, y1 * world_scale);

        // UV coordinates from glyph rect
        let [u0, v0, uw, vh] = glyph.uv_rect;
        let u1 = u0 + uw;
        let v1 = v0 + vh;

        // Two triangles per quad (counter-clockwise winding)
        // Triangle 1: bottom-left, bottom-right, top-left
        vertices.push(TexturedVertex3D { position: p00, uv: [u0, v1], color });
        vertices.push(TexturedVertex3D { position: p10, uv: [u1, v1], color });
        vertices.push(TexturedVertex3D { position: p01, uv: [u0, v0], color });

        // Triangle 2: bottom-right, top-right, top-left
        vertices.push(TexturedVertex3D { position: p10, uv: [u1, v1], color });
        vertices.push(TexturedVertex3D { position: p11, uv: [u1, v0], color });
        vertices.push(TexturedVertex3D { position: p01, uv: [u0, v0], color });

        cursor_x += glyph.advance;
    }

    vertices
}

/// 3D placement + styling for [`generate_text_vertices_aligned`].
pub struct TextStyle3D {
    /// The 3D position (anchor point depends on alignment).
    pub position: [f32; 3],
    /// Scale factor.
    pub scale: f32,
    /// RGBA color.
    pub color: [f32; 4],
    /// The "right" direction vector.
    pub right: [f32; 3],
    /// The "up" direction vector.
    pub up: [f32; 3],
    /// Horizontal alignment: -1.0 = left, 0.0 = center, 1.0 = right.
    pub align: f32,
}

/// Generates vertices for text with horizontal alignment.
///
/// # Arguments
/// * `text` - The text string to render
/// * `font_atlas` - The font atlas containing glyph information
/// * `style` - 3D placement + styling ([`TextStyle3D`])
pub fn generate_text_vertices_aligned(
    text: &str,
    font_atlas: &FontAtlas,
    style: TextStyle3D,
) -> Vec<TexturedVertex3D> {
    let TextStyle3D {
        position,
        scale,
        color,
        right,
        up,
        align,
    } = style;
    let text_width = measure_text_width(text, font_atlas);
    let world_scale = scale / font_atlas.base_font_size;

    // Calculate offset based on alignment
    let offset_x = -text_width * (align + 1.0) * 0.5 * world_scale;

    let adjusted_position = [
        position[0] + right[0] * offset_x,
        position[1] + right[1] * offset_x,
        position[2] + right[2] * offset_x,
    ];

    generate_text_vertices(text, adjusted_position, font_atlas, scale, color, right, up)
}

/// Measures the width of a text string in font units (before scaling).
pub fn measure_text_width(text: &str, font_atlas: &FontAtlas) -> f32 {
    let mut width = 0.0f32;
    let mut max_width = 0.0f32;

    for ch in text.chars() {
        if ch == '\n' {
            max_width = max_width.max(width);
            width = 0.0;
            continue;
        }

        if ch == '\r' {
            continue;
        }

        if let Some(glyph) = font_atlas.glyphs.get(&ch) {
            let advance = if ch == '\t' { glyph.advance * 4.0 } else { glyph.advance };
            width += advance;
        }
    }

    max_width.max(width)
}

/// Measures the height of a text string in font units (before scaling).
/// Returns (total_height, line_count)
pub fn measure_text_height(text: &str, font_atlas: &FontAtlas) -> (f32, u32) {
    let line_count = text.lines().count().max(1) as u32;
    let total_height = font_atlas.line_height * line_count as f32;
    (total_height, line_count)
}

/// Transforms a 2D point (in text-local space) to 3D world space.
#[inline]
pub(crate) fn transform_point(
    origin: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    x: f32,
    y: f32,
) -> [f32; 3] {
    [
        origin[0] + right[0] * x + up[0] * y,
        origin[1] + right[1] * x + up[1] * y,
        origin[2] + right[2] * x + up[2] * y,
    ]
}

/// Creates a simple ASCII font atlas glyph map for a fixed-width font.
/// This is a helper for creating basic font atlases from grid-based textures.
///
/// # Arguments
/// * `chars_per_row` - Number of characters per row in the atlas
/// * `char_width` - Width of each character cell in pixels
/// * `char_height` - Height of each character cell in pixels
/// * `atlas_width` - Total atlas width in pixels
/// * `atlas_height` - Total atlas height in pixels
/// * `first_char` - ASCII code of the first character (usually 32 for space)
/// * `last_char` - ASCII code of the last character (usually 126 for ~)
///
/// # Returns
/// A HashMap of character to GlyphInfo suitable for register_font_atlas.
pub fn create_fixed_width_glyph_map(
    chars_per_row: u32,
    char_width: f32,
    char_height: f32,
    atlas_width: f32,
    atlas_height: f32,
    first_char: u8,
    last_char: u8,
) -> std::collections::HashMap<char, GlyphInfo> {
    let mut glyphs = std::collections::HashMap::new();

    let cell_u = char_width / atlas_width;
    let cell_v = char_height / atlas_height;

    for code in first_char..=last_char {
        let index = (code - first_char) as u32;
        let col = index % chars_per_row;
        let row = index / chars_per_row;

        let u = col as f32 * cell_u;
        let v = row as f32 * cell_v;

        glyphs.insert(
            code as char,
            GlyphInfo {
                uv_rect: [u, v, cell_u, cell_v],
                bearing_x: 0.0,
                bearing_y: char_height * 0.8, // Approximate baseline
                width: char_width,
                height: char_height,
                advance: char_width,
            },
        );
    }

    glyphs
}

/// Adds a textured quad to the vertex list.
/// Useful for building custom text geometry.
#[inline]
pub fn add_text_quad(
    vertices: &mut Vec<TexturedVertex3D>,
    p00: [f32; 3], // bottom-left
    p10: [f32; 3], // bottom-right
    p01: [f32; 3], // top-left
    p11: [f32; 3], // top-right
    uv_rect: [f32; 4], // [u, v, width, height]
    color: [f32; 4],
) {
    let [u0, v0, uw, vh] = uv_rect;
    let u1 = u0 + uw;
    let v1 = v0 + vh;

    // Triangle 1
    vertices.push(TexturedVertex3D { position: p00, uv: [u0, v1], color });
    vertices.push(TexturedVertex3D { position: p10, uv: [u1, v1], color });
    vertices.push(TexturedVertex3D { position: p01, uv: [u0, v0], color });

    // Triangle 2
    vertices.push(TexturedVertex3D { position: p10, uv: [u1, v1], color });
    vertices.push(TexturedVertex3D { position: p11, uv: [u1, v0], color });
    vertices.push(TexturedVertex3D { position: p01, uv: [u0, v0], color });
}
