use crate::embedder;
use super::overlay_impl::on_present;

/// Build a software‐renderer config with the *present* callback.
pub(crate) fn build_software_renderer_config() -> embedder::FlutterRendererConfig {
    let mut sw: embedder::FlutterSoftwareRendererConfig = unsafe { std::mem::zeroed() };
    sw.struct_size = std::mem::size_of::<embedder::FlutterSoftwareRendererConfig>();
    sw.surface_present_callback = Some(on_present);

    let mut cfg: embedder::FlutterRendererConfig = unsafe { std::mem::zeroed() };
    cfg.type_ = embedder::FlutterRendererType_kSoftware;
    cfg.__bindgen_anon_1.software = sw;
    cfg
}
