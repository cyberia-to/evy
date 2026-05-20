//! `Diagnostic` — accumulator for per-system measurements.
//!
//! The dispatch scheduler (or any system) records `SystemMetrics` here;
//! periodic drains feed telemetry, profilers, or the φ*-budget tuner.

use std::collections::HashMap;

/// One measurement of one named system. Aggregated by name in `Diagnostic`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemMetrics {
    pub name: &'static str,
    pub wall_ns: u64,
    pub cycles: u64,
    pub instructions: u64,
    pub l1d_misses: u64,
    pub branch_misses: u64,
    /// Number of recorded calls — 1 for a fresh `Timer::stop` result,
    /// higher after aggregation in `Diagnostic::record`.
    pub call_count: u64,
}

impl SystemMetrics {
    /// Combine two measurements of the same system. Wall/cycle counts
    /// sum; call_count increments. Caller's responsibility to ensure the
    /// `name` fields match.
    pub fn merge(&mut self, other: &SystemMetrics) {
        debug_assert_eq!(
            self.name, other.name,
            "merging measurements from different systems"
        );
        self.wall_ns = self.wall_ns.saturating_add(other.wall_ns);
        self.cycles = self.cycles.saturating_add(other.cycles);
        self.instructions = self.instructions.saturating_add(other.instructions);
        self.l1d_misses = self.l1d_misses.saturating_add(other.l1d_misses);
        self.branch_misses = self.branch_misses.saturating_add(other.branch_misses);
        self.call_count = self.call_count.saturating_add(other.call_count);
    }

    /// Average wall-clock nanoseconds per call.
    pub fn avg_wall_ns(&self) -> u64 {
        if self.call_count == 0 {
            0
        } else {
            self.wall_ns / self.call_count
        }
    }

    /// Average CPU cycles per call. Returns 0 in Wall-only mode.
    pub fn avg_cycles(&self) -> u64 {
        if self.call_count == 0 {
            0
        } else {
            self.cycles / self.call_count
        }
    }
}

/// Per-name accumulator. Held as a resource by the engine; the dispatch
/// scheduler records into it after each frame; periodic drains feed
/// telemetry sinks.
#[derive(Debug, Default)]
pub struct Diagnostic {
    by_name: HashMap<&'static str, SystemMetrics>,
}

impl Diagnostic {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one measurement. Merges with any existing entry under the same name.
    pub fn record(&mut self, m: SystemMetrics) {
        match self.by_name.get_mut(m.name) {
            Some(existing) => existing.merge(&m),
            None => {
                self.by_name.insert(m.name, m);
            }
        }
    }

    /// Look up the aggregated metrics for a system, if any.
    pub fn get(&self, name: &str) -> Option<&SystemMetrics> {
        self.by_name.get(name)
    }

    /// Iterate aggregated metrics for every recorded system, sorted by
    /// total wall time descending — the hottest systems come first.
    pub fn ranked(&self) -> Vec<&SystemMetrics> {
        let mut v: Vec<&SystemMetrics> = self.by_name.values().collect();
        v.sort_by(|a, b| b.wall_ns.cmp(&a.wall_ns));
        v
    }

    /// Clear all accumulated measurements. Call at the start of a new
    /// measurement window.
    pub fn reset(&mut self) {
        self.by_name.clear();
    }

    /// Number of distinct systems recorded.
    pub fn system_count(&self) -> usize {
        self.by_name.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &'static str, wall_ns: u64, cycles: u64) -> SystemMetrics {
        SystemMetrics {
            name,
            wall_ns,
            cycles,
            call_count: 1,
            ..Default::default()
        }
    }

    #[test]
    fn record_first_sample_inserts() {
        let mut d = Diagnostic::new();
        d.record(sample("a", 100, 1000));
        assert_eq!(d.get("a").unwrap().wall_ns, 100);
        assert_eq!(d.get("a").unwrap().call_count, 1);
    }

    #[test]
    fn record_subsequent_samples_merge() {
        let mut d = Diagnostic::new();
        d.record(sample("a", 100, 1000));
        d.record(sample("a", 200, 2000));
        d.record(sample("a", 300, 3000));
        let m = d.get("a").unwrap();
        assert_eq!(m.wall_ns, 600);
        assert_eq!(m.cycles, 6000);
        assert_eq!(m.call_count, 3);
        assert_eq!(m.avg_wall_ns(), 200);
        assert_eq!(m.avg_cycles(), 2000);
    }

    #[test]
    fn ranked_is_descending_by_wall_time() {
        let mut d = Diagnostic::new();
        d.record(sample("cheap", 10, 0));
        d.record(sample("expensive", 1000, 0));
        d.record(sample("medium", 100, 0));
        let r = d.ranked();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].name, "expensive");
        assert_eq!(r[1].name, "medium");
        assert_eq!(r[2].name, "cheap");
    }

    #[test]
    fn reset_clears_all() {
        let mut d = Diagnostic::new();
        d.record(sample("a", 100, 0));
        d.record(sample("b", 200, 0));
        assert_eq!(d.system_count(), 2);
        d.reset();
        assert_eq!(d.system_count(), 0);
        assert!(d.get("a").is_none());
    }

    #[test]
    fn missing_system_returns_none() {
        let d = Diagnostic::new();
        assert!(d.get("never_recorded").is_none());
    }

    #[test]
    fn avg_with_zero_calls_returns_zero() {
        let m = SystemMetrics::default();
        assert_eq!(m.avg_wall_ns(), 0);
        assert_eq!(m.avg_cycles(), 0);
    }
}
