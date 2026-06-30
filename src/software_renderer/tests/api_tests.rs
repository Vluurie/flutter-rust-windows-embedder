use crate::software_renderer::api::{FlutterEmbedderError, should_skip_resize};

#[test]
fn skip_resize_when_unchanged_and_not_forced() {
    assert!(should_skip_resize((0, 0, 800, 600), (0, 0, 800, 600), false));
}

#[test]
fn no_skip_when_forced() {
    assert!(!should_skip_resize((0, 0, 800, 600), (0, 0, 800, 600), true));
}

#[test]
fn no_skip_when_size_changed() {
    assert!(!should_skip_resize((0, 0, 800, 600), (0, 0, 1920, 1080), false));
}

#[test]
fn no_skip_when_position_changed() {
    assert!(!should_skip_resize((0, 0, 800, 600), (10, 20, 800, 600), false));
}

#[test]
fn error_display_messages() {
    assert_eq!(
        FlutterEmbedderError::InitializationFailed("x".to_string()).to_string(),
        "Flutter Initialization Failed: x"
    );
    assert_eq!(
        FlutterEmbedderError::OperationFailed("y".to_string()).to_string(),
        "Flutter Operation Failed: y"
    );
    assert_eq!(
        FlutterEmbedderError::EngineNotRunning.to_string(),
        "Flutter engine is not running or handle is null."
    );
    assert_eq!(
        FlutterEmbedderError::InvalidHandle.to_string(),
        "Invalid Flutter overlay handle provided."
    );
}
