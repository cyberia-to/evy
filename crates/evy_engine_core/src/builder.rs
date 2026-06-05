//! Builder for `Engine`. Lets callers opt subsystems in or out.

use evy_diagnostic::{Diagnostic, PmuProbe, ProbeMode};
use evy_ecs_storage::ShardStorage;
use evy_engine_dispatch::{DispatchError, DispatchScheduler, PlatformCapabilities};
use evy_radio::RadioClient;
use evy_dialect::DialectRegistry;

use crate::engine::Engine;

/// Fluent builder for [`Engine`]. Most subsystems are opt-out: by default
/// the builder enables all of them. Toggle off via `without_*` for unit
/// tests or specialized configurations.
pub struct EngineBuilder {
    enable_radio: bool,
    pmu_mode: ProbeMode,
    /// Override the auto-probed capabilities. Used in tests to fabricate
    /// scenarios (e.g. "what if AMX is missing").
    capabilities: Option<PlatformCapabilities>,
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self {
            enable_radio: true,
            pmu_mode: ProbeMode::Wall,
            capabilities: None,
        }
    }
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Do not spawn a radio daemon. Use this for fully-offline engines
    /// (testing, headless tools).
    pub fn without_radio(mut self) -> Self {
        self.enable_radio = false;
        self
    }

    /// Request PMU mode for the diagnostic probe. Falls back to Wall on
    /// failure (no root / no entitlement / not Apple Silicon).
    pub fn with_pmu(mut self) -> Self {
        self.pmu_mode = ProbeMode::Pmu;
        self
    }

    /// Override the platform capability probe with an explicit value.
    /// Useful in tests; production engines should let the engine probe
    /// the actual host.
    pub fn with_capabilities(mut self, caps: PlatformCapabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }

    /// Build the engine. Initializes every subsystem and runs the
    /// platform probe.
    pub fn build(self) -> Result<Engine, EngineInitError> {
        let capabilities = self
            .capabilities
            .unwrap_or_else(PlatformCapabilities::probe);

        let storage = ShardStorage::from_capabilities(&capabilities);
        let scheduler =
            DispatchScheduler::new(capabilities.clone()).map_err(EngineInitError::Dispatch)?;
        let radio = if self.enable_radio {
            Some(RadioClient::spawn().map_err(EngineInitError::Radio)?)
        } else {
            None
        };
        let dialect_registry = DialectRegistry::new();
        let probe = PmuProbe::new(self.pmu_mode);
        let diagnostic = Diagnostic::new();

        Ok(Engine::from_parts(
            capabilities,
            storage,
            scheduler,
            radio,
            dialect_registry,
            probe,
            diagnostic,
        ))
    }
}

/// Errors during engine construction.
#[derive(Debug)]
pub enum EngineInitError {
    Dispatch(DispatchError),
    Radio(evy_radio::RadioError),
}

impl core::fmt::Display for EngineInitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Dispatch(e) => write!(f, "dispatch init failed: {e}"),
            Self::Radio(e) => write!(f, "radio init failed: {e}"),
        }
    }
}

impl std::error::Error for EngineInitError {}
