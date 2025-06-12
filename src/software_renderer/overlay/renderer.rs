use super::overlay_impl::on_present;
use crate::bindings::embedder::{self, FlutterRendererConfig__bindgen_ty_1 as RendererConfig};

/// Build a software‐renderer config with the *present* callback.
pub(crate) fn build_software_renderer_config() -> embedder::FlutterRendererConfig {
    let sw = embedder::FlutterSoftwareRendererConfig {
        struct_size: std::mem::size_of::<embedder::FlutterSoftwareRendererConfig>(),
        surface_present_callback: Some(on_present),
    };

    embedder::FlutterRendererConfig {
        type_: embedder::FlutterRendererType_kSoftware,
        __bindgen_anon_1: RendererConfig { software: sw },
    }
}
