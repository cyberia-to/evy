//! evy_radio — the channel bridge to a radio (iroh) P2P daemon.
//!
//! Per `evy/specs/evy.md` §9: iroh runs in its own tokio runtime in a
//! dedicated thread. Bevy holds a `RadioClient` (just channel handles).
//! Async iroh operations are submitted as messages; completions arrive
//! on the return channel which Bevy polls non-blockingly at frame start.
//!
//! This isolates iroh's `select!`-heavy async code from Bevy's executor
//! and avoids the cancellation-safety pitfalls documented in the cyber
//! agent memory (`feedback_select_cancel_safety`).
//!
//! Session 11.1 scope:
//! - Crate skeleton with the channel-bridge API
//! - Request/Response enum shapes
//! - Stub daemon thread (no actual iroh integration yet)
//! - Tests demonstrating round-trip submit → poll
//!
//! Session 11.2 adds the actual iroh blob fetch + gossip subscribe.
//! Session 11.3 wires the radio:// AssetSource scheme to `ShardStorage`'s
//! network backend.

mod client;
mod daemon;
mod message;

pub use client::{RadioClient, RadioError, RequestId};
pub use daemon::DaemonHandle;
pub use message::{GossipEvent, RadioRequest, RadioResponse};
