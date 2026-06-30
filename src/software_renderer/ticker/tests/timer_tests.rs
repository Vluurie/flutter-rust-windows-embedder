use crate::software_renderer::ticker::task_runner_window::Timer;
use std::time::Duration;

#[test]
fn fresh_timer_has_no_deadline() {
    let t = Timer::new();
    assert_eq!(t.current_deadline(), None);
}

#[test]
fn schedule_arms_deadline() {
    let t = Timer::new();
    t.schedule_in(Duration::from_millis(100));
    assert_eq!(t.current_deadline(), Some(Duration::from_millis(100)));
}

#[test]
fn earlier_deadline_replaces_later() {
    let t = Timer::new();
    t.schedule_in(Duration::from_millis(200));
    t.schedule_in(Duration::from_millis(50));
    assert_eq!(t.current_deadline(), Some(Duration::from_millis(50)));
}

#[test]
fn later_deadline_is_ignored() {
    let t = Timer::new();
    t.schedule_in(Duration::from_millis(50));
    t.schedule_in(Duration::from_millis(200));
    assert_eq!(t.current_deadline(), Some(Duration::from_millis(50)));
}

#[test]
fn equal_deadline_is_ignored() {
    let t = Timer::new();
    t.schedule_in(Duration::from_millis(100));
    t.schedule_in(Duration::from_millis(100));
    assert_eq!(t.current_deadline(), Some(Duration::from_millis(100)));
}
