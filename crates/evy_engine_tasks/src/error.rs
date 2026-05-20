//! Errors returnable by AMX and ANE task pools.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolError {
    /// The target hardware does not support this engine
    /// (e.g., AMX on x86 or Android, ANE on Linux).
    Unsupported,
    /// The pool has been shut down. Workers joined; no new tasks accepted.
    ShutDown,
    /// AMX context initialization failed on a worker thread. Probe
    /// `PlatformCapabilities` before constructing the pool to avoid this.
    AmxInitFailed,
    /// ANE rane runtime initialization or program load failed.
    AneRuntimeFailed(String),
}

impl core::fmt::Display for PoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported => write!(f, "pool not supported on this target"),
            Self::ShutDown => write!(f, "pool shut down; no new tasks accepted"),
            Self::AmxInitFailed => write!(f, "AMX context initialization failed"),
            Self::AneRuntimeFailed(msg) => write!(f, "ANE runtime error: {msg}"),
        }
    }
}

impl std::error::Error for PoolError {}
