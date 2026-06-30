use crate::software_renderer::multiview::resize_decision::{
    can_open_shared_texture, needs_host_texture_recreate, should_copy_frame,
    should_realloc_texture, should_request_resize,
};

#[test]
fn realloc_when_config_size_differs() {
    assert!(should_realloc_texture((2560, 1400), (1600, 1000)));
}

#[test]
fn no_realloc_when_sizes_equal() {
    assert!(!should_realloc_texture((1600, 1000), (1600, 1000)));
}

#[test]
fn realloc_on_partial_size_change() {
    assert!(should_realloc_texture((1600, 1400), (1600, 1000)));
}

#[test]
fn request_resize_when_client_grew() {
    assert!(should_request_resize(
        2560,
        1400,
        1600,
        1000,
        Some((1600, 1000))
    ));
}

#[test]
fn request_resize_when_engine_texture_lags() {
    assert!(should_request_resize(
        2560,
        1400,
        2560,
        1400,
        Some((1600, 1000))
    ));
}

#[test]
fn request_resize_when_engine_texture_missing() {
    assert!(should_request_resize(2560, 1400, 2560, 1400, None));
}

#[test]
fn no_request_resize_when_everything_matches() {
    assert!(!should_request_resize(
        2560,
        1400,
        2560,
        1400,
        Some((2560, 1400))
    ));
}

#[test]
fn can_open_only_when_engine_matches_target() {
    assert!(can_open_shared_texture(Some((2560, 1400)), 2560, 1400));
}

#[test]
fn cannot_open_when_engine_size_lags() {
    assert!(!can_open_shared_texture(Some((1600, 1000)), 2560, 1400));
}

#[test]
fn cannot_open_when_engine_size_missing() {
    assert!(!can_open_shared_texture(None, 2560, 1400));
}

#[test]
fn copy_only_on_new_frame() {
    assert!(should_copy_frame(5, 4));
    assert!(!should_copy_frame(5, 5));
    assert!(!should_copy_frame(4, 5));
}

#[test]
fn host_recreate_when_texture_grew() {
    assert!(needs_host_texture_recreate((2560, 1400), (1600, 1000)));
    assert!(!needs_host_texture_recreate((1600, 1000), (1600, 1000)));
}

#[test]
fn maximize_sequence_converges() {
    let target = (2560u32, 1400u32);
    let mut cur = (1600u32, 1000u32);
    let mut engine_tex = (1600u32, 1000u32);

    assert!(should_request_resize(
        target.0,
        target.1,
        cur.0,
        cur.1,
        Some(engine_tex)
    ));
    cur = target;

    assert!(!can_open_shared_texture(Some(engine_tex), cur.0, cur.1));
    assert!(should_request_resize(
        target.0,
        target.1,
        cur.0,
        cur.1,
        Some(engine_tex)
    ));

    assert!(should_realloc_texture(target, engine_tex));
    engine_tex = target;

    assert!(can_open_shared_texture(Some(engine_tex), cur.0, cur.1));
    assert!(!should_request_resize(
        target.0,
        target.1,
        cur.0,
        cur.1,
        Some(engine_tex)
    ));
    assert!(!should_realloc_texture(target, engine_tex));
}
