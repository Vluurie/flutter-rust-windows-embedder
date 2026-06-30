//! Public value types used by the overlay manager API. Split out of `mod.rs`
//! to keep that file focused on the manager + handle implementations.

use std::collections::HashMap;

use windows::Win32::Graphics::Direct3D11::{ID3D11SamplerState, ID3D11ShaderResourceView};

use crate::software_renderer::d3d11_compositor::text_3d_renderer::GlyphInfo;

/// A font atlas to register on an overlay for 3D text rendering.
pub struct FontAtlasSpec {
    /// Unique identifier for this font (used in subsequent text calls).
    pub font_id: String,
    /// The font atlas texture as a shader resource view.
    pub texture: ID3D11ShaderResourceView,
    /// The sampler state for the texture.
    pub sampler: ID3D11SamplerState,
    /// Map of characters to their glyph information.
    pub glyphs: HashMap<char, GlyphInfo>,
    /// The height of a line in font units (normalized).
    pub line_height: f32,
    /// The base font size in pixels (used for scaling).
    pub base_font_size: f32,
}

/// Specifies which rendering pass to execute, allowing for separation of 3D
/// primitives and 2D UI.
pub enum FlutterRenderPass {
    /// Render only the 3D primitives.
    PrimitivesOnly,
    /// Render only the 2D Flutter UI.
    UiOnly,
}
