//! evy_diagnostic — per-system measurements for the dispatch DAG.
//!
//! Two modes, picked at runtime:
//!
//! - **wall** — `std::time::Instant`-based. Always available. The default.
//! - **pmu** — hardware PMU counters via [`acpu::pulse::Counters`].
//!   Requires Apple Silicon, the `pmu` feature flag, AND running as root
//!   or with the `dtrace_proc` entitlement. Falls back to wall if probe
//!   fails.
//!
//! A `Diagnostic` instance accumulates per-name samples; the scheduler or
//! application drains it periodically into telemetry sinks.

mod diagnostic;
mod probe;
mod timer;

pub use diagnostic::{Diagnostic, SystemMetrics};
pub use probe::{ProbeMode, PmuProbe, Snapshot, SnapshotDelta};
pub use timer::Timer;
