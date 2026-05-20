//! `Timer` — RAII-style measurement scope.
//!
//! Typical use:
//!
//! ```ignore
//! let mut probe = PmuProbe::new(ProbeMode::Wall);
//! let timer = Timer::start(&probe, "transform_propagate");
//! // ... do work ...
//! let metrics = timer.stop(&probe);
//! diagnostic.record(metrics);
//! ```

use crate::diagnostic::SystemMetrics;
use crate::probe::{PmuProbe, Snapshot};

/// In-flight measurement. Created with `Timer::start`; consumed by `stop`.
pub struct Timer {
    name: &'static str,
    start: Snapshot,
}

impl Timer {
    /// Begin measuring under `name`. The label is propagated into the
    /// resulting `SystemMetrics` for aggregation by the diagnostic.
    pub fn start(probe: &PmuProbe, name: &'static str) -> Self {
        Self {
            name,
            start: probe.snapshot(),
        }
    }

    /// Stop measuring. Returns a `SystemMetrics` carrying the delta
    /// since `start`.
    pub fn stop(self, probe: &PmuProbe) -> SystemMetrics {
        let end = probe.snapshot();
        let delta = probe.delta(&self.start, &end);
        SystemMetrics {
            name: self.name,
            wall_ns: delta.wall_ns,
            cycles: delta.cycles,
            instructions: delta.instructions,
            l1d_misses: delta.l1d_misses,
            branch_misses: delta.branch_misses,
            call_count: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::ProbeMode;

    #[test]
    fn timer_records_label_and_elapsed() {
        let probe = PmuProbe::new(ProbeMode::Wall);
        let t = Timer::start(&probe, "test_system");
        std::thread::sleep(std::time::Duration::from_millis(1));
        let m = t.stop(&probe);
        assert_eq!(m.name, "test_system");
        assert!(m.wall_ns > 0);
        assert_eq!(m.call_count, 1);
    }
}
