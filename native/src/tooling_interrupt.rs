//! Scoped launcher cancellation; never installs a persistent Tokio signal handler.
#![allow(unsafe_code)]
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub(crate) struct Interrupt {
    #[cfg(unix)]
    previous: libc::sigaction,
    #[cfg(windows)]
    registered: bool,
    closed: bool,
}

#[cfg(unix)]
extern "C" fn on_interrupt(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

#[cfg(windows)]
unsafe extern "system" fn on_interrupt(event: u32) -> i32 {
    if event > 1 {
        return 0;
    }
    INTERRUPTED.store(true, Ordering::SeqCst);
    1
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn GetConsoleCP() -> u32;
}

#[cfg(windows)]
pub(crate) fn console_attached() -> bool {
    // SAFETY: process-local query with no pointer or ownership transfer.
    unsafe { GetConsoleCP() != 0 }
}

impl Interrupt {
    pub(crate) fn install() -> Result<Self, String> {
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("another foreground action owns launcher cancellation".into());
        }
        INTERRUPTED.store(false, Ordering::SeqCst);
        let result = Self::register();
        if result.is_err() {
            ACTIVE.store(false, Ordering::SeqCst);
        }
        result
    }

    fn register() -> Result<Self, String> {
        #[cfg(unix)]
        {
            // SAFETY: libc supplies the platform ABI; both output structures are initialized.
            // The callback performs only a lock-free atomic store, with no allocation or I/O.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                let mut previous: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = on_interrupt as *const () as libc::sighandler_t;
                action.sa_flags = libc::SA_RESTART;
                if libc::sigemptyset(&mut action.sa_mask) != 0
                    || libc::sigaction(libc::SIGINT, &action, &mut previous) != 0
                {
                    return Err("cannot register launcher interruption handler".into());
                }
                Ok(Self {
                    previous,
                    closed: false,
                })
            }
        }
        #[cfg(windows)]
        {
            let registered = console_attached();
            // A redirected/headless command has no console events. Do not create a
            // visible console merely to install a handler for an event it cannot receive.
            // SAFETY: static callback, no borrowed state; never inherit ignore-Ctrl+C.
            if registered && unsafe { SetConsoleCtrlHandler(Some(on_interrupt), 1) } == 0 {
                return Err("cannot register launcher interruption handler".into());
            }
            Ok(Self {
                registered,
                closed: false,
            })
        }
        #[cfg(not(any(unix, windows)))]
        Err("launcher interruption is unsupported on this platform".into())
    }

    pub(crate) fn interrupted(&self) -> bool {
        INTERRUPTED.load(Ordering::SeqCst)
    }

    pub(crate) async fn wait(&self) -> Result<(), String> {
        while !INTERRUPTED.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Ok(())
    }

    pub(crate) fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        #[cfg(unix)]
        // SAFETY: exactly restore the captured action after the owned child is finalized.
        let restored =
            unsafe { libc::sigaction(libc::SIGINT, &self.previous, std::ptr::null_mut()) == 0 };
        #[cfg(windows)]
        // SAFETY: remove only this static callback, never reset other process handlers.
        let restored =
            !self.registered || unsafe { SetConsoleCtrlHandler(Some(on_interrupt), 0) != 0 };
        #[cfg(not(any(unix, windows)))]
        let restored = false;
        if !restored {
            return Err("cannot restore launcher interruption handling".into());
        }
        self.closed = true;
        ACTIVE.store(false, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("ERROR: {error}");
        }
    }
}
