use crate::bindings::embedder::FlutterTask;
use crate::software_renderer::ticker::task_scheduler::{SafeFlutterTask, ScheduledTask};
use std::collections::BinaryHeap;
use std::ptr;

fn task(target_time: u64, id: u64) -> ScheduledTask {
    ScheduledTask {
        task: SafeFlutterTask(FlutterTask {
            runner: ptr::null_mut(),
            task: id,
        }),
        target_time,
    }
}

#[test]
fn heap_pops_earliest_target_time_first() {
    let mut heap = BinaryHeap::new();
    heap.push(task(100, 1));
    heap.push(task(50, 2));
    heap.push(task(200, 3));
    heap.push(task(75, 4));

    assert_eq!(heap.pop().unwrap().target_time, 50);
    assert_eq!(heap.pop().unwrap().target_time, 75);
    assert_eq!(heap.pop().unwrap().target_time, 100);
    assert_eq!(heap.pop().unwrap().target_time, 200);
}

#[test]
fn equal_times_are_ordered_by_task_id() {
    let mut heap = BinaryHeap::new();
    heap.push(task(100, 9));
    heap.push(task(100, 3));
    let a = heap.pop().unwrap();
    let b = heap.pop().unwrap();
    assert_eq!(a.target_time, 100);
    assert_eq!(b.target_time, 100);
    assert_ne!(a.task.0.task, b.task.0.task);
}

#[test]
fn single_task_round_trips() {
    let mut heap = BinaryHeap::new();
    heap.push(task(42, 7));
    let popped = heap.pop().unwrap();
    assert_eq!(popped.target_time, 42);
    assert_eq!(popped.task.0.task, 7);
    assert!(heap.is_empty());
}
