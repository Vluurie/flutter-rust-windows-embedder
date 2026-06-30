use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG, PostMessageW,
    RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_NULL, WNDCLASSW,
};
use windows::core::PCWSTR;

/// Cross-thread waker for the platform task runner. Holds the runner's hidden
/// message-window handle and posts `WM_NULL` to it to break the runner out of
/// `GetMessage`, exactly like Flutter's `TaskRunnerWindow::WakeUp`.
pub struct Waker {
    hwnd: AtomicIsize,
    posted: AtomicBool,
}

impl Waker {
    pub fn new() -> Self {
        Self {
            hwnd: AtomicIsize::new(0),
            posted: AtomicBool::new(false),
        }
    }

    pub fn wake_up(&self) {
        let raw = self.hwnd.load(Ordering::Acquire);
        if raw == 0 {
            return;
        }
        if self
            .posted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let _ =
                unsafe { PostMessageW(Some(HWND(raw as *mut _)), WM_NULL, WPARAM(0), LPARAM(0)) };
        }
    }

    fn set_hwnd(&self, hwnd: HWND) {
        self.hwnd.swap(hwnd.0 as isize, Ordering::Release);
    }

    fn clear_posted(&self) {
        self.posted.store(false, Ordering::Release);
    }
}

impl Default for Waker {
    fn default() -> Self {
        Self::new()
    }
}

/// Fires the message loop once after a requested delay, mirroring Flutter's
/// `TimerThread`. A dedicated thread parks on a condvar until the earliest
/// requested deadline, then calls `wake_up()` on the runner. Re-arming with an
/// earlier deadline preempts the current wait; later deadlines are ignored while
/// an earlier one is pending. This avoids the busy-spin of polling the queue.
pub struct Timer {
    inner: Mutex<TimerState>,
    cv: Condvar,
}

struct TimerState {
    deadline: Option<Duration>,
    generation: u64,
    stop: bool,
}

impl Timer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(TimerState {
                deadline: None,
                generation: 0,
                stop: false,
            }),
            cv: Condvar::new(),
        })
    }

    /// Requests a wake-up no later than `delay` from now. A pending earlier
    /// deadline is kept; an equal-or-later one is ignored.
    pub fn schedule_in(&self, delay: Duration) {
        let mut state = self.inner.lock().unwrap();
        match state.deadline {
            Some(existing) if existing <= delay => {}
            _ => {
                state.deadline = Some(delay);
                state.generation = state.generation.wrapping_add(1);
                self.cv.notify_all();
            }
        }
    }

    /// The currently armed wake-up delay, if any.
    pub(crate) fn current_deadline(&self) -> Option<Duration> {
        self.inner.lock().unwrap().deadline
    }

    pub fn run(self: &Arc<Self>, waker: Arc<Waker>) {
        let timer = Arc::clone(self);
        std::thread::spawn(move || {
            loop {
                let wait = {
                    let mut state = timer.inner.lock().unwrap();
                    if state.stop {
                        return;
                    }
                    match state.deadline.take() {
                        Some(d) => {
                            let gen_before = state.generation;
                            let (mut guard, timeout) = timer.cv.wait_timeout(state, d).unwrap();
                            if guard.stop {
                                return;
                            }
                            if timeout.timed_out() && guard.generation == gen_before {
                                drop(guard);
                                waker.wake_up();
                            } else {
                                guard.deadline.get_or_insert(d);
                            }
                            continue;
                        }
                        None => state,
                    }
                };
                let guard = timer.cv.wait(wait).unwrap();
                drop(guard);
            }
        });
    }
}

const RUNNER_CLASS: PCWSTR = windows::core::w!("FlutterTaskRunnerWindow");

static CLASS_REGISTERED: OnceLock<()> = OnceLock::new();

fn register_runner_class() {
    CLASS_REGISTERED.get_or_init(|| unsafe {
        let hinst = GetModuleHandleW(None).map(|h| h.into()).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(runner_wnd_proc),
            hInstance: hinst,
            lpszClassName: RUNNER_CLASS,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
}

thread_local! {
    static PROCESS_FN: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
        const { std::cell::RefCell::new(None) };
}

extern "system" fn runner_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NULL {
        PROCESS_FN.with(|f| {
            if let Ok(mut slot) = f.try_borrow_mut()
                && let Some(cb) = slot.as_mut()
            {
                cb();
            }
        });
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Creates the hidden message window on the calling thread, stores its handle in
/// `waker`, and runs the `GetMessage` loop until quit. On every `WM_NULL` wake it
/// clears the flood guard and invokes `process`.
pub fn run_message_loop(waker: &Waker, mut process: impl FnMut() + 'static) {
    register_runner_class();

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            RUNNER_CLASS,
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
    };
    let hwnd = match hwnd {
        Ok(h) if !h.0.is_null() => h,
        _ => return,
    };

    waker.set_hwnd(hwnd);

    let waker_ptr: *const Waker = waker;
    PROCESS_FN.with(|f| {
        *f.borrow_mut() = Some(Box::new(move || {
            unsafe { &*waker_ptr }.clear_posted();
            process();
        }));
    });

    let mut msg = MSG::default();
    loop {
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
