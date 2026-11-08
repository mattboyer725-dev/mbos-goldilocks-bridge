use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use crate::AdaptiveLoop;

/// Local closed-loop runner.
/// Feeds the AdaptiveLoop from its own prior state. No external input, no network.
pub struct ClosedLoopEngine {
    loop_: AdaptiveLoop,
    is_running: Arc<AtomicBool>,
}

impl ClosedLoopEngine {
    pub fn new(node_id: &str, target: u64) -> (Self, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(true));
        let engine = Self {
            loop_: AdaptiveLoop::new(node_id, target),
            is_running: flag.clone(),
        };
        (engine, flag)
    }

    /// One cycle: derive a local seed from current state and step.
    pub fn execute_cycle(&mut self) {
        let state = self.loop_.current_state();
        let seed = state.state.wrapping_add(state.generation);
        self.loop_.step(seed);
    }

    pub fn run_continuous(mut self, interval_ms: u64) {
        while self.is_running.load(Ordering::Relaxed) {
            self.execute_cycle();
            thread::sleep(Duration::from_millis(interval_ms));
        }
    }

    pub fn current_generation(&self) -> u64 {
        self.loop_.current_state().generation
    }
}
