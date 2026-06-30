//! # D3D11 compositor
//!
//! The D3D11 rendering pieces an overlay draws on top of (or alongside) the
//! Flutter UI: world-space 3D primitives, 3D text, and screen post-processing.
//!
//! These are normally driven through [`FlutterOverlay`] or
//! [`FlutterOverlayManagerHandle`] rather than used directly. The renderers all
//! follow the same submit / latch / draw lifecycle:
//!
//! 1. Submit geometry into a named group (for example
//!    [`FlutterOverlay::set_primitives`] or [`FlutterOverlay::set_text`]). This
//!    replaces whatever was in that group.
//! 2. Latch once per frame (for example
//!    [`FlutterOverlay::latch_queued_primitives`]) to snapshot the submitted
//!    geometry into the buffers that will be drawn. Submitting and drawing happen
//!    on different threads, so the latch is the hand-off point.
//! 3. Draw, done for you inside the manager's per-frame
//!    [`FlutterOverlayManagerHandle::render_primitives`] /
//!    [`FlutterOverlayManagerHandle::render_ui`].
//!
//! ## Submodules
//!
//! * [`primitive_3d_renderer`]: triangles and lines ([`primitive_3d_renderer::Vertex3D`]),
//!   blend/depth options ([`primitive_3d_renderer::PrimitiveOptions`]), and custom
//!   pixel-shader effects.
//! * [`primitive_presets`]: helpers that build common shapes (boxes, spheres,
//!   lines) into `Vertex3D` buffers.
//! * [`text_3d_renderer`]: font-atlas-based 3D text
//!   ([`text_3d_renderer::TexturedVertex3D`], [`text_3d_renderer::GlyphInfo`]).
//! * [`text_presets`]: builds text vertices from a string and a font atlas.
//! * [`effects`]: post-processing effect configuration (hologram, warp field,
//!   glitch) applied to the composited UI.
//! * [`post_processing_renderer`]: the renderer that applies those effects.
//! * [`traits`]: the shared [`traits::Renderer`] interface and per-frame
//!   [`traits::FrameParams`].
//!
//! [`FlutterOverlay`]: crate::software_renderer::api::FlutterOverlay
//! [`FlutterOverlay::set_primitives`]: crate::software_renderer::api::FlutterOverlay::set_primitives
//! [`FlutterOverlay::set_text`]: crate::software_renderer::api::FlutterOverlay::set_text
//! [`FlutterOverlay::latch_queued_primitives`]: crate::software_renderer::api::FlutterOverlay::latch_queued_primitives
//! [`FlutterOverlayManagerHandle`]: crate::software_renderer::overlays_manager_api::FlutterOverlayManagerHandle
//! [`FlutterOverlayManagerHandle::render_primitives`]: crate::software_renderer::overlays_manager_api::FlutterOverlayManagerHandle::render_primitives
//! [`FlutterOverlayManagerHandle::render_ui`]: crate::software_renderer::overlays_manager_api::FlutterOverlayManagerHandle::render_ui

pub mod effects;
pub mod post_processing_renderer;
pub mod primitive_3d_renderer;
pub mod primitive_presets;
pub mod text_3d_renderer;
pub mod text_presets;
pub mod traits;
#[cfg(test)]
mod tests;
