use std::time::{Duration, Instant};
use windows::Win32::System::{
    ProcessStatus::{EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2},
    Threading::GetCurrentProcess,
};

const STARTUP_SETTLE: Duration = Duration::from_secs(1);
const IDLE_SETTLE: Duration = Duration::from_secs(2);
const LOW_RESIDENT_BYTES: usize = 12 * 1024 * 1024;

/// One trim after startup or after a return to idle. A frame or clock tick
/// must never rearm this: repeatedly evicting active pages hurts playback.
pub struct WorkingSet {
    startup: bool,
    pending: bool,
    quiet_since: Option<Instant>,
    focused: bool,
}

impl WorkingSet {
    pub fn new() -> Self {
        Self {
            startup: true,
            pending: true,
            quiet_since: None,
            focused: false,
        }
    }

    pub fn update(&mut self, now: Instant, busy: bool, focused: bool) -> Option<Instant> {
        let lost_focus = self.focused && !focused;
        self.focused = focused;
        if busy || lost_focus {
            self.pending = true;
            self.quiet_since = None;
        }
        if busy || !self.pending {
            return None;
        }
        let settle = if self.startup {
            STARTUP_SETTLE
        } else {
            IDLE_SETTLE
        };
        let deadline = *self.quiet_since.get_or_insert(now) + settle;
        if now < deadline {
            return Some(deadline);
        }
        self.pending = false;
        self.quiet_since = None;
        let startup = std::mem::take(&mut self.startup);
        trim(startup);
        None
    }
}

fn trim(startup: bool) {
    let process = unsafe { GetCurrentProcess() };
    if !startup {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX2::default();
        counters.cb = std::mem::size_of_val(&counters) as u32;
        let measured = unsafe {
            K32GetProcessMemoryInfo(
                process,
                (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2).cast(),
                counters.cb,
            )
        }
        .as_bool();
        if measured && counters.PrivateWorkingSetSize <= LOW_RESIDENT_BYTES {
            return;
        }
    }
    let phase = if startup { "Startup" } else { "Idle" };
    let status = unsafe { EmptyWorkingSet(process) };
    println!(
        "{phase} working set trim: {}",
        if status.is_ok() { "done" } else { "failed" }
    );
}
