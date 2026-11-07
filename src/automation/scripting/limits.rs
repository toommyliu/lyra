use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use rquickjs::runtime::InterruptHandler;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) turn: Duration,
    pub(crate) memory: usize,
    pub(crate) stack: usize,
    pub(crate) stop_grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            turn: Duration::from_secs(5),
            memory: 64 << 20,
            stack: 1 << 20,
            stop_grace: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tripped {
    Stop,
    Turn,
}

const BUSY_AT_STOP: Duration = Duration::from_millis(50);

pub(super) struct Clock {
    epoch: Instant,
    turn_started: AtomicU64,
    stop_requested: AtomicU64,
    stop: AtomicBool,
    tripped: AtomicU8,
}

impl Clock {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            epoch: Instant::now(),
            turn_started: AtomicU64::new(0),
            stop_requested: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            tripped: AtomicU8::new(0),
        })
    }

    fn now(&self) -> u64 {
        (self.epoch.elapsed().as_nanos() as u64).max(1)
    }

    pub(super) fn turn<T>(&self, run: impl FnOnce() -> T) -> T {
        self.turn_started.store(self.now(), Ordering::SeqCst);
        let result = run();
        self.turn_started.store(0, Ordering::SeqCst);
        result
    }

    pub(super) fn request_stop(&self) {
        let _ =
            self.stop_requested
                .compare_exchange(0, self.now(), Ordering::SeqCst, Ordering::SeqCst);
    }

    pub(super) fn stop_now(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub(super) fn tripped(&self) -> Option<Tripped> {
        match self.tripped.load(Ordering::SeqCst) {
            1 => Some(Tripped::Stop),
            2 => Some(Tripped::Turn),
            _ => None,
        }
    }

    pub(super) fn interrupt_handler(self: &Arc<Self>, limits: &Limits) -> InterruptHandler {
        let clock = self.clone();
        let turn_limit = limits.turn.as_nanos() as u64;
        let grace = limits.stop_grace.as_nanos() as u64;
        let busy = BUSY_AT_STOP.as_nanos() as u64;
        Box::new(move || {
            let now = clock.now();
            let started = clock.turn_started.load(Ordering::SeqCst);
            let requested = clock.stop_requested.load(Ordering::SeqCst);
            let stopping = requested != 0
                && (now.saturating_sub(requested) > grace
                    || (started != 0 && started < requested && now - requested > busy));
            let tripped = if clock.stop.load(Ordering::SeqCst) || stopping {
                1
            } else if started != 0 && now.saturating_sub(started) > turn_limit {
                2
            } else {
                0
            };
            if tripped != 0 {
                let _ =
                    clock
                        .tripped
                        .compare_exchange(0, tripped, Ordering::SeqCst, Ordering::SeqCst);
            }
            tripped != 0
        })
    }
}
