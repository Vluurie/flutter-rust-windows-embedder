use crate::software_renderer::gl_renderer::angle_interop::{
    EGL_NONE, build_display_attributes, egl_error_to_string,
};

#[test]
fn egl_error_known_codes() {
    assert_eq!(egl_error_to_string(0x3000), "EGL_SUCCESS");
    assert_eq!(egl_error_to_string(0x3001), "EGL_NOT_INITIALIZED");
    assert_eq!(egl_error_to_string(0x300E), "EGL_CONTEXT_LOST");
    assert_eq!(egl_error_to_string(0x3008), "EGL_BAD_DISPLAY");
}

#[test]
fn egl_error_unknown_code() {
    assert_eq!(egl_error_to_string(0x9999), "Unknown EGL error");
    assert_eq!(egl_error_to_string(-1), "Unknown EGL error");
}

#[test]
fn display_attributes_terminated_with_none() {
    let attrs = build_display_attributes();
    assert!(!attrs.is_empty());
    assert_eq!(*attrs.last().unwrap(), EGL_NONE);
    assert_eq!(attrs.iter().filter(|&&a| a == EGL_NONE).count(), 1);
}
