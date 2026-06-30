/// True when the engine's requested backing-store size differs from the allocated
/// shared-texture size, so the texture must be reallocated.
pub(crate) fn should_realloc_texture(config_size: (u32, u32), texture_size: (u32, u32)) -> bool {
    config_size != texture_size
}

/// True when new window metrics must be sent: the client size changed or the
/// engine texture size has not yet reached the client size.
pub(crate) fn should_request_resize(
    new_w: u32,
    new_h: u32,
    cur_w: u32,
    cur_h: u32,
    engine_tex_size: Option<(u32, u32)>,
) -> bool {
    new_w != cur_w || new_h != cur_h || engine_tex_size != Some((new_w, new_h))
}

/// True when the engine texture size equals the target swapchain size, so it is
/// safe to open the shared texture for blitting.
pub(crate) fn can_open_shared_texture(
    engine_size: Option<(u32, u32)>,
    target_w: u32,
    target_h: u32,
) -> bool {
    engine_size == Some((target_w, target_h))
}

/// True when the engine has presented a frame newer than the last copied one.
pub(crate) fn should_copy_frame(presented: u64, copied: u64) -> bool {
    presented > copied
}

/// True when the host compositing texture size no longer matches the view's
/// current shared-texture size.
pub(crate) fn needs_host_texture_recreate(
    texture_size: (u32, u32),
    host_texture_size: (u32, u32),
) -> bool {
    texture_size != host_texture_size
}
