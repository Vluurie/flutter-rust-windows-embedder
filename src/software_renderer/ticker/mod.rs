//! # Frame loop and task running (internal)
//!
//! Each overlay runs a dedicated task-runner thread that drives the Flutter
//! engine: it executes tasks the engine schedules, delivers queued platform
//! messages and key events, and pumps view-focus changes. The thread sleeps on a
//! timer between deadlines instead of busy-waiting.
//!
//! On the software path, the engine hands rendered pixels back through
//! [`on_present`], which copies them into the overlay's pixel buffer; the next
//! [`tick`](crate::software_renderer::api::FlutterOverlay::tick) uploads that
//! buffer to the D3D11 texture.
//!
//! This module is an internal implementation detail and is not part of the public
//! API; it is documented here for contributors.

pub mod present;
pub mod spawn;
pub mod task_runner_window;
pub mod task_scheduler;
#[allow(clippy::module_inception)]
pub mod ticker;
#[cfg(test)]
mod tests;
pub use present::on_present;
