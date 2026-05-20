//! Runtime platform capability probe for evy.
//!
//! Detects which compute engines and storage backends are available on the
//! host. The dispatch scheduler routes work to available engines and respects
//! per-system `FallbackPolicy` declarations.
//!
//! See `evy/specs/evy.md` §3.4 for the conceptual model and §5 for the
//! engine taxonomy.

use std::collections::HashSet;

/// A distinct dispatch context the evy scheduler must track.
///
/// NEON is intentionally NOT a separate engine — it shares the Cpu dispatch
/// context (any thread can issue NEON in the same instruction stream as
/// scalar). AMX is its own engine because per-thread `AmxCtx` must be
/// initialized. ANE has its own dispatch queue via rane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Engine {
    /// Scalar + NEON SIMD on P-cores/E-cores. Always available.
    Cpu,
    /// Apple AMX matrix coprocessor. Per-cluster, per-thread context required.
    Amx,
    /// GPU via wgpu (always) or aruminium-direct (Apple Silicon only).
    Gpu(GpuPath),
    /// Apple Neural Engine via rane (MIL bytecode). Or NNAPI on Android.
    Ane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuPath {
    /// Cross-platform raster + compute via wgpu (Metal/Vulkan/D3D12).
    Wgpu,
    /// Direct Metal compute via honeycrisp/aruminium. Apple Silicon only.
    Aruminium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpuKind {
    /// Apple Neural Engine.
    Ane,
    /// Android Neural Networks API.
    Nnapi,
    /// AMD Versal AI Engine.
    Xdna,
    /// Qualcomm Hexagon Neural network processor.
    Hexagon,
}

/// Memory architecture class.
///
/// Determines whether ShardStore can default to a zero-copy backend
/// (`unimem` on Apple Silicon, `AHardwareBuffer` on Android — TBD) or must
/// use the `memory` backend with explicit upload to GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryClass {
    /// Unified memory: CPU/GPU/NPU see the same physical pages.
    Uma,
    /// UMA but with reduced bandwidth vs reference platform.
    UmaBandwidthLimited,
    /// Discrete GPU; staging-buffer cost on cross-engine transfers.
    NonUma,
}

/// Thermal class of the host platform. Sets the default tick rate and
/// budget headroom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalBudget {
    /// Plugged-in workstation. Sustained inference acceptable.
    Desktop,
    /// Battery laptop. Bursty inference preferred.
    Laptop,
    /// Tablet form factor. Aggressive throttling on sustained load.
    Tablet,
    /// Phone. Inference is rate-limited per second.
    Phone,
    /// XR headset. Strict per-frame budget; no thermal headroom.
    Xr,
}

/// What the scheduler does when a dispatch node's preferred engine is
/// unavailable on the current platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackPolicy {
    /// Refuse to load if the preferred engine is missing.
    Required,
    /// Run on the specified alternate engine with reduced quality.
    DegradeTo(Engine),
    /// Omit the node from the dispatch graph silently.
    Skip,
}

/// Snapshot of host platform capabilities at startup.
///
/// Cheap to clone (Copy-shaped except for `engines: HashSet`). Held as a
/// Bevy `Resource` by `evy_engine_core`; queried by every backend-selecting
/// system and every dispatch node's `engine()` resolution.
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    pub engines: HashSet<Engine>,
    pub has_unimem: bool,
    pub has_amx: bool,
    pub has_ane: bool,
    pub npu_kind: Option<NpuKind>,
    pub gpu_paths: HashSet<GpuPath>,
    pub memory_class: MemoryClass,
    pub thermal_budget: ThermalBudget,
    /// Suggested gameplay-state cadence (BBG commit frequency) derived from
    /// thermal class. Caller may override.
    pub tick_rate_hz: u32,
}

impl PlatformCapabilities {
    /// Probe the current host. Cheap; runs once at startup.
    pub fn probe() -> Self {
        let has_unimem = probe_unimem();
        let has_amx = is_apple_silicon();
        let has_ane = is_apple_silicon();
        let mut gpu_paths = HashSet::new();
        gpu_paths.insert(GpuPath::Wgpu);
        if is_apple_silicon() {
            gpu_paths.insert(GpuPath::Aruminium);
        }
        let mut engines = HashSet::new();
        engines.insert(Engine::Cpu);
        if has_amx {
            engines.insert(Engine::Amx);
        }
        for &p in &gpu_paths {
            engines.insert(Engine::Gpu(p));
        }
        if has_ane {
            engines.insert(Engine::Ane);
        }

        let memory_class = if has_unimem {
            MemoryClass::Uma
        } else if cfg!(target_os = "android") {
            // Android phones/tablets have UMA but lower bandwidth than M-series.
            MemoryClass::UmaBandwidthLimited
        } else {
            MemoryClass::NonUma
        };

        let thermal_budget = if cfg!(target_os = "android") {
            ThermalBudget::Phone
        } else if cfg!(target_os = "macos") {
            // macOS laptops and desktops — conservative default; caller
            // can refine using IOKit if needed.
            ThermalBudget::Laptop
        } else {
            ThermalBudget::Desktop
        };

        let tick_rate_hz = match thermal_budget {
            ThermalBudget::Desktop => 30,
            ThermalBudget::Laptop => 20,
            ThermalBudget::Tablet => 15,
            ThermalBudget::Phone => 10,
            ThermalBudget::Xr => 90, // tracking-rate frame budget; tick = frame on XR
        };

        let npu_kind = if has_ane {
            Some(NpuKind::Ane)
        } else if cfg!(target_os = "android") {
            // assume NNAPI present on modern Android; explicit probe deferred
            Some(NpuKind::Nnapi)
        } else {
            None
        };

        Self {
            engines,
            has_unimem,
            has_amx,
            has_ane,
            npu_kind,
            gpu_paths,
            memory_class,
            thermal_budget,
            tick_rate_hz,
        }
    }

    /// True if all engines required by the policy are present.
    pub fn supports(&self, engine: Engine) -> bool {
        self.engines.contains(&engine)
    }

    /// Resolve a fallback policy against current capabilities to the engine
    /// the scheduler should actually use, or `None` if the policy refuses.
    pub fn resolve(&self, preferred: Engine, policy: FallbackPolicy) -> Option<Engine> {
        if self.supports(preferred) {
            return Some(preferred);
        }
        match policy {
            FallbackPolicy::Required => None,
            FallbackPolicy::DegradeTo(alt) => {
                if self.supports(alt) {
                    Some(alt)
                } else {
                    None
                }
            }
            FallbackPolicy::Skip => None,
        }
    }
}

const fn is_apple_silicon() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

#[cfg(feature = "probe-unimem")]
fn probe_unimem() -> bool {
    // The honest probe: try to allocate a minimum-size IOSurface-pinned block.
    // Block::open returns Err on non-Apple-Silicon or if IOSurface is
    // unavailable (sandboxed environments, etc.).
    unimem::Block::open(8).is_ok()
}

#[cfg(not(feature = "probe-unimem"))]
fn probe_unimem() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_always_available() {
        let caps = PlatformCapabilities::probe();
        assert!(caps.supports(Engine::Cpu));
        assert!(caps.gpu_paths.contains(&GpuPath::Wgpu));
    }

    #[test]
    fn tick_rate_matches_thermal_class() {
        let caps = PlatformCapabilities::probe();
        let expected = match caps.thermal_budget {
            ThermalBudget::Desktop => 30,
            ThermalBudget::Laptop => 20,
            ThermalBudget::Tablet => 15,
            ThermalBudget::Phone => 10,
            ThermalBudget::Xr => 90,
        };
        assert_eq!(caps.tick_rate_hz, expected);
    }

    #[test]
    fn resolve_returns_preferred_when_available() {
        let caps = PlatformCapabilities::probe();
        assert_eq!(
            caps.resolve(Engine::Cpu, FallbackPolicy::Required),
            Some(Engine::Cpu)
        );
    }

    #[test]
    fn resolve_required_returns_none_when_missing() {
        // Construct a capabilities snapshot that doesn't have Amx,
        // independent of the host. Validates FallbackPolicy::Required
        // semantics directly.
        let caps = PlatformCapabilities {
            engines: [Engine::Cpu].into_iter().collect(),
            has_unimem: false,
            has_amx: false,
            has_ane: false,
            npu_kind: None,
            gpu_paths: [GpuPath::Wgpu].into_iter().collect(),
            memory_class: MemoryClass::NonUma,
            thermal_budget: ThermalBudget::Desktop,
            tick_rate_hz: 30,
        };
        assert_eq!(caps.resolve(Engine::Amx, FallbackPolicy::Required), None);
    }

    #[test]
    fn resolve_degrade_falls_back() {
        let caps = PlatformCapabilities {
            engines: [Engine::Cpu].into_iter().collect(),
            has_unimem: false,
            has_amx: false,
            has_ane: false,
            npu_kind: None,
            gpu_paths: [GpuPath::Wgpu].into_iter().collect(),
            memory_class: MemoryClass::NonUma,
            thermal_budget: ThermalBudget::Desktop,
            tick_rate_hz: 30,
        };
        assert_eq!(
            caps.resolve(Engine::Amx, FallbackPolicy::DegradeTo(Engine::Cpu)),
            Some(Engine::Cpu)
        );
    }

    #[test]
    fn resolve_skip_returns_none_for_missing() {
        let caps = PlatformCapabilities {
            engines: [Engine::Cpu].into_iter().collect(),
            has_unimem: false,
            has_amx: false,
            has_ane: false,
            npu_kind: None,
            gpu_paths: [GpuPath::Wgpu].into_iter().collect(),
            memory_class: MemoryClass::NonUma,
            thermal_budget: ThermalBudget::Desktop,
            tick_rate_hz: 30,
        };
        assert_eq!(caps.resolve(Engine::Ane, FallbackPolicy::Skip), None);
    }
}
