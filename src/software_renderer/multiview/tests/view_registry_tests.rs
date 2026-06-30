use crate::software_renderer::multiview::ViewRegistry;

#[test]
fn allocate_id_starts_at_one() {
    let reg = ViewRegistry::new();
    assert_eq!(reg.allocate_id(), 1);
}

#[test]
fn allocate_id_is_monotonic() {
    let reg = ViewRegistry::new();
    let a = reg.allocate_id();
    let b = reg.allocate_id();
    let c = reg.allocate_id();
    assert_eq!((a, b, c), (1, 2, 3));
}

#[test]
fn allocate_id_concurrent_is_unique() {
    use std::collections::HashSet;
    use std::sync::Arc;

    let reg = Arc::new(ViewRegistry::new());
    let mut handles = Vec::new();
    for _ in 0..8 {
        let r = reg.clone();
        handles.push(std::thread::spawn(move || {
            (0..100).map(|_| r.allocate_id()).collect::<Vec<_>>()
        }));
    }
    let mut all = HashSet::new();
    for h in handles {
        for id in h.join().unwrap() {
            assert!(all.insert(id), "duplicate id {id}");
        }
    }
    assert_eq!(all.len(), 800);
}

#[test]
fn empty_registry_state() {
    let reg = ViewRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
    assert!(reg.view_ids().is_empty());
}

#[test]
fn with_view_on_missing_returns_none() {
    let reg = ViewRegistry::new();
    let result = reg.with_view(42, |_| 1);
    assert!(result.is_none());
}

#[test]
fn remove_missing_returns_none() {
    let reg = ViewRegistry::new();
    assert!(reg.remove(42).is_none());
}
