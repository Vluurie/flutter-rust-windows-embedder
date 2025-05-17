use crate::embedder::FlutterTask;

use log::{error, info, warn};
use once_cell::sync::Lazy;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct SafeFlutterTask(pub FlutterTask);

unsafe impl Send for SafeFlutterTask {}
unsafe impl Sync for SafeFlutterTask {}

#[derive(Debug, Copy, Clone)]
pub struct ScheduledTask {
    pub task: SafeFlutterTask,

    pub target_time: u64,
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .target_time
            .cmp(&self.target_time)
            .then_with(|| self.task.0.task.cmp(&other.task.0.task))
    }
}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.target_time == other.target_time && self.task.0.task == other.task.0.task
    }
}

impl Eq for ScheduledTask {}

#[derive(Debug, Default, Clone)]
pub struct TaskRunnerContext {
    pub task_runner_thread_id: Option<ThreadId>,
}

pub static TASK_QUEUE_STATE: Lazy<Arc<TaskQueueState>> = Lazy::new(|| {
    info!("[TaskScheduler] Initializing global TASK_QUEUE_STATE.");
    Arc::new(TaskQueueState {
        queue: Mutex::new(BinaryHeap::new()),
        condvar: Condvar::new(),
    })
});

pub struct TaskQueueState {
    pub queue: Mutex<BinaryHeap<ScheduledTask>>,
    pub condvar: Condvar,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn post_task_callback(
    task: FlutterTask,
    target_time_nanos: u64,
    _user_data: *mut c_void,
) {
    let scheduled_task = ScheduledTask {
        task: SafeFlutterTask(task),
        target_time: target_time_nanos,
    };

    let state = &*TASK_QUEUE_STATE;
    match state.queue.lock() {
        Ok(mut queue_guard) => {
            queue_guard.push(scheduled_task);
            state.condvar.notify_one();
        }
        Err(poisoned) => {
            error!(
                "[TaskScheduler] post_task_callback: Task queue mutex was poisoned! {:?}",
                poisoned
            );
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn runs_task_on_current_thread_callback(user_data: *mut c_void) -> bool {
    if user_data.is_null() {
        error!("[TaskScheduler] runs_task_on_current_thread_callback: user_data is null.");
        return false;
    }
    let context = unsafe { &*(user_data as *const TaskRunnerContext) };

    match context.task_runner_thread_id {
        Some(runner_thread_id) => {
            let current_thread_id = std::thread::current().id();
            current_thread_id == runner_thread_id
        }
        None => {
            warn!(
                "[TaskScheduler] runs_task_on_current_thread_callback: Task runner thread ID not set in context."
            );
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn destroy_task_runner_context_callback(user_data: *mut c_void) {
    if !user_data.is_null() {
        drop(unsafe { Box::from_raw(user_data as *mut TaskRunnerContext) });
        info!("[TaskScheduler] TaskRunnerContext destroyed and memory freed.");
    } else {
        info!("[TaskScheduler] destroy_task_runner_context_callback called with null user_data.");
    }
}
