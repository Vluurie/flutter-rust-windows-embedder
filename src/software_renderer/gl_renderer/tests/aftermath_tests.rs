use crate::software_renderer::gl_renderer::nvidia_aftermath::{AftermathResult, CrashDumpStatus};

#[test]
fn aftermath_result_known_codes() {
    assert_eq!(AftermathResult::from(0x1), AftermathResult::Success);
    assert_eq!(AftermathResult::from(0x2), AftermathResult::NotAvailable);
    assert_eq!(AftermathResult::from(0xBAD00000), AftermathResult::Fail);
    assert_eq!(AftermathResult::from(0xBAD00002), AftermathResult::NotInitialized);
    assert_eq!(AftermathResult::from(0xBAD00016), AftermathResult::Disabled);
}

#[test]
fn aftermath_result_unknown_code() {
    assert_eq!(AftermathResult::from(0xDEADBEEF), AftermathResult::Unknown);
    assert_eq!(AftermathResult::from(0), AftermathResult::Unknown);
}

#[test]
fn crash_dump_status_known_codes() {
    assert_eq!(CrashDumpStatus::from(1), CrashDumpStatus::NotStarted);
    assert_eq!(CrashDumpStatus::from(2), CrashDumpStatus::Collecting);
    assert_eq!(CrashDumpStatus::from(3), CrashDumpStatus::Finished);
    assert_eq!(CrashDumpStatus::from(4), CrashDumpStatus::InvokingCallback);
}

#[test]
fn crash_dump_status_unknown_code() {
    assert_eq!(CrashDumpStatus::from(0), CrashDumpStatus::Unknown);
    assert_eq!(CrashDumpStatus::from(99), CrashDumpStatus::Unknown);
}
