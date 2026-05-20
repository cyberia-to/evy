//! Errors returnable by the dispatch scheduler.

use evy_platform_caps::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// A node declared an engine the current platform does not support
    /// and its `FallbackPolicy` does not permit a fallback.
    EngineUnavailable(Engine),
    /// The dependency graph contains a cycle. Reads/writes have a circular
    /// dependency; either the topology is wrong or a node misdeclares its
    /// access set.
    CycleDetected,
    /// A required engine pool (AmxTaskPool / AneTaskPool) failed to
    /// initialize at scheduler construction. The dispatch DAG cannot start.
    PoolInitFailed(String),
}

impl core::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EngineUnavailable(e) => write!(f, "engine unavailable: {e:?}"),
            Self::CycleDetected => write!(f, "dispatch DAG contains a cycle"),
            Self::PoolInitFailed(msg) => write!(f, "engine pool init failed: {msg}"),
        }
    }
}

impl std::error::Error for DispatchError {}
