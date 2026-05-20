//! Engine-specific task pools for evy: `AmxTaskPool` and `AneTaskPool`.
//!
//! Bevy's own task pools (`ComputeTaskPool`, `AsyncComputeTaskPool`,
//! `IoTaskPool`) serve the `Cpu` engine. These pools cover the two engines
//! that need a dispatch context Bevy doesn't know about:
//!
//! - [`AmxTaskPool`] — N worker threads, each with an `acpu::matrix::Matrix`
//!   initialized for its lifetime. AMX context is per-thread; tasks
//!   submitted to this pool run on a worker that already has AMX active.
//!
//! - [`AneTaskPool`] — a single worker thread that drains an ANE program
//!   queue. ANE is a coprocessor shared by the OS; concurrent dispatch
//!   serializes in hardware, so we serialize at the API boundary.
//!
//! On non-Apple-Silicon targets the pools compile but every operation
//! returns `PoolError::Unsupported`. The dispatch scheduler (later step)
//! consults `PlatformCapabilities` before routing work to either pool.

mod amx;
mod ane;
mod error;

pub use amx::AmxTaskPool;
pub use ane::AneTaskPool;
pub use error::PoolError;
