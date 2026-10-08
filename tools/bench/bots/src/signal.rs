//! Ctrl-C (and SIGTERM on Unix) only raise [`STOP`]: the bots see it within a tick, close their
//! sockets, and the report is still written.

use std::sync::atomic::{AtomicBool, Ordering};

pub static STOP: AtomicBool = AtomicBool::new(false);

pub fn stop_requested() -> bool {
    STOP.load(Ordering::Relaxed)
}

pub fn request_stop() {
    STOP.store(true, Ordering::Relaxed);
}

/// Installs the handler. Returns false if the system refused it; Ctrl-C then kills the process
/// without a report.
pub fn install() -> bool {
    imp::install()
}

#[cfg(windows)]
mod imp {
    type HandlerRoutine = unsafe extern "system" fn(ctrl_type: u32) -> i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<HandlerRoutine>, add: i32) -> i32;
    }

    unsafe extern "system" fn on_ctrl(_ctrl_type: u32) -> i32 {
        super::request_stop();
        // Handled: the process keeps running and shuts down by itself.
        1
    }

    pub fn install() -> bool {
        // SAFETY: registers a handler that only stores to an atomic.
        unsafe { SetConsoleCtrlHandler(Some(on_ctrl), 1) != 0 }
    }
}

#[cfg(unix)]
mod imp {
    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    const SIG_ERR: usize = usize::MAX;

    extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }

    extern "C" fn on_signal(_signum: i32) {
        // An atomic store is async-signal-safe.
        super::request_stop();
    }

    pub fn install() -> bool {
        let handler = on_signal as extern "C" fn(i32) as usize;
        // SAFETY: the handler only stores to an atomic.
        unsafe { signal(SIGINT, handler) != SIG_ERR && signal(SIGTERM, handler) != SIG_ERR }
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    pub fn install() -> bool {
        false
    }
}
