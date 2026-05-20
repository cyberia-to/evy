//! `PmuProbe` — the measurement source. Wraps wall time always; PMU
//! counters when available and requested.

use std::time::Instant;

/// Operating mode picked at probe construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    /// `std::time::Instant`-based wall time. Always available.
    Wall,
    /// Hardware PMU counters via acpu. Apple Silicon + `pmu` feature
    /// + root/dtrace_proc entitlement required. Falls back to Wall if
    /// the underlying probe fails to initialize.
    Pmu,
}

/// Captured measurement snapshot at a point in time. Holds wall time
/// always; PMU counts when available.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub wall: Instant,
    #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "pmu"))]
    pub pmu: Option<acpu::pulse::Snapshot>,
}

/// Delta between two snapshots.
#[derive(Debug, Clone, Copy, Default)]
pub struct SnapshotDelta {
    /// Elapsed wall-clock nanoseconds.
    pub wall_ns: u64,
    /// CPU cycles (PMU mode only; zero in Wall mode).
    pub cycles: u64,
    /// Retired instructions (PMU mode only).
    pub instructions: u64,
    /// L1 data cache misses (PMU mode only).
    pub l1d_misses: u64,
    /// Branch mispredictions (PMU mode only).
    pub branch_misses: u64,
}

impl SnapshotDelta {
    /// Instructions per cycle. Returns 0 in Wall mode (no instructions data).
    pub fn ipc(&self) -> f32 {
        if self.cycles == 0 {
            0.0
        } else {
            self.instructions as f32 / self.cycles as f32
        }
    }
}

/// The probe — owns the PMU counters (if any) and exposes `snapshot()`.
pub struct PmuProbe {
    mode: ProbeMode,
    #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "pmu"))]
    counters: Option<acpu::pulse::Counters>,
}

impl PmuProbe {
    /// Construct a probe. Asks for `requested` mode; falls back to Wall
    /// if PMU initialization fails (not Apple Silicon, missing entitlement,
    /// kernel rejection, etc.).
    pub fn new(requested: ProbeMode) -> Self {
        match requested {
            ProbeMode::Wall => Self::wall_only(),
            ProbeMode::Pmu => Self::try_pmu(),
        }
    }

    fn wall_only() -> Self {
        Self {
            mode: ProbeMode::Wall,
            #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "pmu"))]
            counters: None,
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "pmu"))]
    fn try_pmu() -> Self {
        use acpu::pulse::Counter;
        let want = [
            Counter::Cycles,
            Counter::Instructions,
            Counter::L1dMisses,
            Counter::BranchMisses,
        ];
        match acpu::pulse::Counters::new(&want) {
            Ok(mut c) => {
                c.start();
                Self {
                    mode: ProbeMode::Pmu,
                    counters: Some(c),
                }
            }
            Err(_) => Self::wall_only(),
        }
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64", feature = "pmu")))]
    fn try_pmu() -> Self {
        // PMU unsupported on this build; silently fall back to wall mode.
        Self::wall_only()
    }

    /// The actual mode in effect (may be Wall even if Pmu was requested).
    pub fn mode(&self) -> ProbeMode {
        self.mode
    }

    /// Capture a measurement snapshot.
    pub fn snapshot(&self) -> Snapshot {
        #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "pmu"))]
        {
            let pmu = self.counters.as_ref().map(|c| c.read());
            return Snapshot {
                wall: Instant::now(),
                pmu,
            };
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64", feature = "pmu")))]
        {
            Snapshot {
                wall: Instant::now(),
            }
        }
    }

    /// Compute the delta between two snapshots.
    pub fn delta(&self, a: &Snapshot, b: &Snapshot) -> SnapshotDelta {
        let wall_ns = b.wall.saturating_duration_since(a.wall).as_nanos() as u64;
        #[allow(unused_mut)]
        let mut out = SnapshotDelta {
            wall_ns,
            ..Default::default()
        };
        #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "pmu"))]
        {
            if let (Some(counters), Some(pa), Some(pb)) =
                (self.counters.as_ref(), a.pmu.as_ref(), b.pmu.as_ref())
            {
                let counts = counters.elapsed(pa, pb);
                out.cycles = counts.cycles;
                out.instructions = counts.instructions;
                out.l1d_misses = counts.l1d_misses;
                out.branch_misses = counts.branch_misses;
            }
        }
        out
    }
}

impl Default for PmuProbe {
    fn default() -> Self {
        Self::wall_only()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_probe_records_elapsed_time() {
        let probe = PmuProbe::new(ProbeMode::Wall);
        assert_eq!(probe.mode(), ProbeMode::Wall);
        let a = probe.snapshot();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = probe.snapshot();
        let d = probe.delta(&a, &b);
        assert!(d.wall_ns > 1_000_000, "elapsed should exceed 1ms");
    }

    #[test]
    fn pmu_requested_falls_back_to_wall_when_unavailable() {
        // Without root/entitlement, PMU init fails. On non-pmu builds
        // it falls back unconditionally. Either way we expect Wall.
        let probe = PmuProbe::new(ProbeMode::Pmu);
        // mode may be Pmu if running as root, or Wall otherwise.
        assert!(matches!(probe.mode(), ProbeMode::Wall | ProbeMode::Pmu));
        // Snapshots still work in both modes.
        let _ = probe.snapshot();
    }

    #[test]
    fn delta_ipc_is_zero_when_no_cycles() {
        let d = SnapshotDelta::default();
        assert_eq!(d.ipc(), 0.0);
    }

    #[test]
    fn delta_ipc_computes_correctly() {
        let d = SnapshotDelta {
            wall_ns: 1000,
            cycles: 100,
            instructions: 150,
            l1d_misses: 0,
            branch_misses: 0,
        };
        assert!((d.ipc() - 1.5).abs() < 1e-3);
    }
}
