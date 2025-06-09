use crate::bindings::embedder::{FlutterCustomTaskRunners, FlutterTask, FlutterTaskRunnerDescription};

use log::{error, warn};
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


#[repr(transparent)]
pub struct SendableCvoid(*mut c_void);

unsafe impl Send for SendableCvoid {}
unsafe impl Sync for SendableCvoid {}

impl From<*mut c_void> for SendableCvoid {
    fn from(ptr: *mut c_void) -> Self {
        SendableCvoid(ptr)
    }
}

impl From<SendableCvoid> for *mut c_void {
    fn from(wrapper: SendableCvoid) -> Self {
        wrapper.0
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct SendableFlutterTaskRunnerDescription(pub FlutterTaskRunnerDescription);

unsafe impl Send for SendableFlutterTaskRunnerDescription {}
unsafe impl Sync for SendableFlutterTaskRunnerDescription {}

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct SendableFlutterCustomTaskRunners(pub FlutterCustomTaskRunners);

unsafe impl Send for SendableFlutterCustomTaskRunners {}
unsafe impl Sync for SendableFlutterCustomTaskRunners {}
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

#[derive(Debug, Clone)]
pub struct TaskRunnerContext {
    pub task_runner_thread_id: Option<ThreadId>,
    pub task_queue: Arc<TaskQueueState>,
}

impl TaskQueueState {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(BinaryHeap::new()),
            condvar: Condvar::new(),
        }
    }
}

#[derive(Debug)]
pub struct TaskQueueState {
    pub queue: Mutex<BinaryHeap<ScheduledTask>>,
    pub condvar: Condvar,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn post_task_callback(
    task: FlutterTask,
    target_time_nanos: u64,
    user_data: *mut c_void, 
) {
    if user_data.is_null() {
        error!("[TaskScheduler] post_task_callback: user_data is null. Task not posted.");
        return;
    }

    let scheduled_task = ScheduledTask {
        task: SafeFlutterTask(task),
        target_time: target_time_nanos,
    };

    let context = unsafe { &*(user_data as *const TaskRunnerContext) };
    let state = &context.task_queue;    

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
        Some(runner_thread_id) => std::thread::current().id() == runner_thread_id,
        None => {
            warn!("[TaskScheduler] runs_task_on_current_thread_callback: Task runner thread ID not set in context.");
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn destroy_task_runner_context_callback(_user_data: *mut c_void) {}