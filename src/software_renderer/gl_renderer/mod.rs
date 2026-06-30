//! # OpenGL / ANGLE rendering path
//!
//! The preferred, hardware-accelerated way an overlay renders. The Flutter engine
//! is given an OpenGL ES renderer backed by ANGLE, which translates GL ES to
//! Direct3D 11. The engine renders into a D3D11 texture that is shared with the
//! host device, so handing a frame to the host is a keyed-mutex acquire/release
//! plus a same-GPU copy, with no CPU round-trip.
//!
//! This path is selected automatically at overlay init when the ANGLE DLLs
//! (`libEGL.dll`, `libGLESv2.dll`) are present; otherwise the embedder falls back
//! to the software renderer. Some features, such as
//! [multi-view satellite windows](crate::software_renderer::multiview), require
//! this path.
//!
//! ## Submodules
//!
//! * [`angle_interop`]: EGL/ANGLE context setup, the Flutter OpenGL renderer
//!   config, and [`angle_interop::SendableAngleState`] (which also handles
//!   D3D device-loss detection and recovery).
//! * [`nvidia_aftermath`]: optional NVIDIA Aftermath GPU crash diagnostics.
//! * `d3d_backup`: fallback resources used when ANGLE init fails.

pub mod angle_interop;
pub mod d3d_backup;
pub mod nvidia_aftermath;
#[cfg(test)]
mod tests;
