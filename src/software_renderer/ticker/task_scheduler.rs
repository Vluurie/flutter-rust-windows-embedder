// task_scheduler.rs

use crate::embedder::{
    self, FlutterTask
};

use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex, Condvar};
use std::cmp::Ordering;
use std::thread::ThreadId;
use std::ffi::c_void;
use once_cell::sync::Lazy;
use log::{error, info, debug, warn};

// --- SafeFlutterTask Wrapper ---
// Wrapper um FlutterTask, um Send und Sync manuell zu implementieren.
// Dies ist notwendig, da FlutterTask rohe Zeiger enthält.
// Wir nehmen an, dass die Flutter Engine die Thread-Sicherheit für die
// FlutterTask-Struktur selbst handhabt, wenn sie über FlutterEngineRunTask
// auf dem korrekten Runner-Thread ausgeführt wird.
#[derive(Debug, Copy, Clone)]
#[repr(transparent)] // Stellt sicher, dass das Layout identisch mit FlutterTask ist
pub struct SafeFlutterTask(pub FlutterTask);

// SAFETY: Wir deklarieren SafeFlutterTask als Send und Sync.
// Dies basiert auf der Annahme, dass die FlutterTask-Instanz selbst
// (insbesondere der `runner`-Zeiger) von der Flutter Engine nur auf dem
// dafür vorgesehenen Task-Runner-Thread verwendet wird, nachdem sie aus der Queue
// geholt und an `FlutterEngineRunTask` übergeben wurde. Der Embedder selbst
// dereferenziert den `runner`-Zeiger nicht auf anderen Threads.
unsafe impl Send for SafeFlutterTask {}
unsafe impl Sync for SafeFlutterTask {} // Sync wird für Lazy<Arc<Mutex<...>>> benötigt

// --- ScheduledTask Definition ---

/// Represents a task posted by the Flutter Engine to be scheduled.
#[derive(Debug, Copy, Clone)]
pub struct ScheduledTask {
    /// The wrapped, thread-safe Flutter task.
    pub task: SafeFlutterTask,
    /// The absolute target time (in nanoseconds, from `FlutterEngineGetCurrentTime()`)
    /// when this task should be executed.
    pub target_time: u64,
}

// Implement `Ord` so `ScheduledTask` can be used in a `BinaryHeap` (min-heap)
// ordered by `target_time`.
impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse the comparison to make `BinaryHeap` act as a min-heap for `target_time`.
        // Tasks with earlier `target_time` have higher priority.
        // If target_time is the same, use the task's opaque u64 identifier for a stable order.
        other.target_time.cmp(&self.target_time)
            .then_with(|| self.task.0.task.cmp(&other.task.0.task))
    }
}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Implement `PartialEq`. `Eq` wird nicht abgeleitet, da FlutterTask kein Eq implementiert.
// Für die Zwecke der BinaryHeap und der Aufgabenverwaltung ist dies ausreichend.
impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        // Compare target time and the internal task identifier for equality.
        // Der `runner`-Zeiger wird hier nicht für die Gleichheit herangezogen.
        self.target_time == other.target_time && self.task.0.task == other.task.0.task
    }
}
// Manuelle Implementierung von Eq, wenn alle Felder PartialEq sind und die Reflexivität etc. gilt.
// Da wir task.runner nicht vergleichen, ist es sicherer, Eq nicht zu implementieren,
// es sei denn, es wird explizit für eine Collection benötigt, die Eq erfordert.
// BinaryHeap benötigt nur Ord (was PartialOrd und PartialEq impliziert).
impl Eq for ScheduledTask {}


// --- TaskRunnerContext ---
#[derive(Debug, Default, Clone)]
pub struct TaskRunnerContext {
    pub task_runner_thread_id: Option<ThreadId>,
}

// --- Global Task Queue State ---
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

// --- Callbacks for the Flutter Engine ---

/// Callback invoked by the Flutter Engine to schedule a task.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn post_task_callback( // `unsafe extern "C"` ist gute Praxis für FFI
    task: FlutterTask, // Original FlutterTask von C
    target_time_nanos: u64,
    _user_data: *mut c_void,
) {

    let scheduled_task = ScheduledTask {
        task: SafeFlutterTask(task), // Wrap the FlutterTask
        target_time: target_time_nanos,
    };

    let state = &*TASK_QUEUE_STATE;
    match state.queue.lock() {
        Ok(mut queue_guard) => {
            queue_guard.push(scheduled_task);
            state.condvar.notify_one();
        }
        Err(poisoned) => {
            error!("[TaskScheduler] post_task_callback: Task queue mutex was poisoned! {:?}", poisoned);
        }
    }
}

/// Callback invoked by the Flutter Engine to check if tasks run on the current thread.
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
            warn!("[TaskScheduler] runs_task_on_current_thread_callback: Task runner thread ID not set in context.");
            false
        }
    }
}

/// Callback for the destruction of the TaskRunnerContext.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn destroy_task_runner_context_callback(user_data: *mut c_void) {
    if !user_data.is_null() {
        // Reconstruct the Box from the raw pointer and let it drop (deallocating the context).
        drop(unsafe { Box::from_raw(user_data as *mut TaskRunnerContext) });
        info!("[TaskScheduler] TaskRunnerContext destroyed and memory freed.");
    } else {
        info!("[TaskScheduler] destroy_task_runner_context_callback called with null user_data.");
    }
}
