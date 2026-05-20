//! Message types exchanged between the Bevy main thread and the radio daemon.

use evy_ecs_storage::ParticleId;

/// Requests Bevy sends to the daemon.
#[derive(Debug, Clone)]
pub enum RadioRequest {
    /// Fetch the bytes of a particle. Daemon resolves via local cache,
    /// peer DHT, and finally the relay fallback. Returns
    /// `RadioResponse::Fetched` on success or `RadioResponse::Error`
    /// on miss/timeout.
    FetchParticle { particle: ParticleId },

    /// Publish bytes as a particle. The daemon hemera-hashes the bytes
    /// to obtain the `ParticleId` and stores+announces via iroh blobs.
    /// Returns `RadioResponse::Published { particle }`.
    PublishParticle { bytes: Vec<u8> },

    /// Subscribe to a gossip topic. The daemon sets up the iroh-gossip
    /// subscription; future messages on this topic surface via
    /// `poll_gossip` as `GossipEvent::Received`.
    Subscribe { topic: String },

    /// Stop receiving messages for a previously subscribed topic.
    Unsubscribe { topic: String },

    /// Publish a payload to a gossip topic. Other peers subscribed to
    /// the same topic receive it via their own gossip stream.
    GossipPublish { topic: String, payload: Vec<u8> },

    /// Graceful shutdown signal. Daemon completes in-flight work and
    /// exits. The daemon is also dropped when `RadioClient` drops.
    Shutdown,
}

/// Responses the daemon sends back, correlated by `RequestId`.
#[derive(Debug, Clone)]
pub enum RadioResponse {
    /// Fetched particle bytes.
    Fetched {
        particle: ParticleId,
        bytes: Vec<u8>,
    },
    /// A particle was published; returned `ParticleId` is the hemera
    /// hash of its bytes (deterministic per content).
    Published { particle: ParticleId },
    /// Subscribe / Unsubscribe acknowledged.
    SubscribeAck { topic: String },
    UnsubscribeAck { topic: String },
    /// Gossip message published (fire-and-forget; no peer-side receipt).
    GossipPublishAck,
    /// Generic error response. `request_id` correlates with the failed
    /// request; daemon-internal errors that aren't tied to a specific
    /// request surface as `DaemonError` separately.
    Error { message: String },
}

/// Push-style gossip events. Not correlated with a `RequestId` — they
/// arrive whenever a subscribed peer publishes to a subscribed topic.
#[derive(Debug, Clone)]
pub enum GossipEvent {
    /// A message arrived on a subscribed topic.
    Received { topic: String, payload: Vec<u8> },
    /// A topic subscription was lost (peer mesh churn). Bevy may want
    /// to re-subscribe.
    SubscriptionLost { topic: String },
}
