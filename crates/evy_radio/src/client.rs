//! `RadioClient` — the Bevy-side handle. Owns the request sender, the
//! response receiver, and the gossip receiver. Drops the daemon when it
//! goes out of scope.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

use crate::daemon::DaemonHandle;
use crate::message::{GossipEvent, RadioRequest, RadioResponse};

/// Monotonically incrementing identifier correlating a request to its
/// response. Wraps a u64 — collisions don't occur within any reasonable
/// session length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);

/// Errors from the client side. Daemon-internal errors surface as
/// `RadioResponse::Error` correlated by request id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioError {
    /// Daemon thread has shut down or panicked.
    DaemonGone,
    /// Spawn failed at construction time.
    SpawnFailed(String),
}

impl core::fmt::Display for RadioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DaemonGone => write!(f, "radio daemon has exited"),
            Self::SpawnFailed(msg) => write!(f, "radio daemon spawn failed: {msg}"),
        }
    }
}

impl std::error::Error for RadioError {}

/// Internal envelope wrapping a request with its correlation id.
pub(crate) struct Envelope {
    pub(crate) id: RequestId,
    pub(crate) request: RadioRequest,
}

/// The Bevy-side handle. Held as a `Resource` by `evy_engine_core`.
///
/// All client methods are non-blocking. `submit` enqueues a request and
/// returns the id immediately; the response surfaces via `poll_responses`
/// at a later frame. Gossip messages surface via `poll_gossip`.
pub struct RadioClient {
    request_tx: mpsc::Sender<Envelope>,
    response_rx: mpsc::Receiver<(RequestId, RadioResponse)>,
    gossip_rx: mpsc::Receiver<GossipEvent>,
    next_id: AtomicU64,
    // Held to keep the daemon thread alive; dropped when client drops.
    _daemon: DaemonHandle,
}

impl RadioClient {
    /// Spawn the daemon and return a client handle. Construction is
    /// infallible unless the OS refuses thread spawn (extremely unlikely).
    pub fn spawn() -> Result<Self, RadioError> {
        let (request_tx, request_rx) = mpsc::channel::<Envelope>();
        let (response_tx, response_rx) = mpsc::channel::<(RequestId, RadioResponse)>();
        let (gossip_tx, gossip_rx) = mpsc::channel::<GossipEvent>();

        let daemon = DaemonHandle::spawn(request_rx, response_tx, gossip_tx);

        Ok(Self {
            request_tx,
            response_rx,
            gossip_rx,
            next_id: AtomicU64::new(1),
            _daemon: daemon,
        })
    }

    /// Submit a request. Returns the request id for correlating with
    /// the eventual `RadioResponse`. Non-blocking.
    pub fn submit(&self, request: RadioRequest) -> Result<RequestId, RadioError> {
        let id = RequestId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let envelope = Envelope {
            id,
            request,
        };
        self.request_tx
            .send(envelope)
            .map_err(|_| RadioError::DaemonGone)?;
        Ok(id)
    }

    /// Drain all completed responses. Non-blocking; returns an empty
    /// `Vec` when nothing has arrived. Call once per frame.
    pub fn poll_responses(&self) -> Vec<(RequestId, RadioResponse)> {
        let mut out = Vec::new();
        while let Ok(resp) = self.response_rx.try_recv() {
            out.push(resp);
        }
        out
    }

    /// Drain all push-style gossip events. Non-blocking; call once per frame.
    pub fn poll_gossip(&self) -> Vec<GossipEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.gossip_rx.try_recv() {
            out.push(ev);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evy_ecs_storage::ParticleId;

    #[test]
    fn fetch_round_trip() {
        let client = RadioClient::spawn().unwrap();
        let p = ParticleId::from_entity(42, 0);
        let id = client.submit(RadioRequest::FetchParticle { particle: p }).unwrap();

        // Poll until we see the response. Daemon should respond quickly.
        let mut tries = 0;
        loop {
            let responses = client.poll_responses();
            if !responses.is_empty() {
                assert_eq!(responses.len(), 1);
                assert_eq!(responses[0].0, id);
                match &responses[0].1 {
                    RadioResponse::Fetched { particle, .. } => assert_eq!(*particle, p),
                    other => panic!("unexpected response: {other:?}"),
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            tries += 1;
            if tries > 1000 {
                panic!("daemon did not respond within 1s");
            }
        }
    }

    #[test]
    fn publish_returns_particle_id() {
        let client = RadioClient::spawn().unwrap();
        let id = client
            .submit(RadioRequest::PublishParticle {
                bytes: vec![1, 2, 3, 4, 5],
            })
            .unwrap();

        let mut tries = 0;
        loop {
            let responses = client.poll_responses();
            if !responses.is_empty() {
                assert_eq!(responses[0].0, id);
                assert!(matches!(
                    responses[0].1,
                    RadioResponse::Published { .. }
                ));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            tries += 1;
            if tries > 1000 {
                panic!("daemon did not respond within 1s");
            }
        }
    }

    #[test]
    fn subscribe_ack() {
        let client = RadioClient::spawn().unwrap();
        let topic = "cyberlinks".to_string();
        let id = client
            .submit(RadioRequest::Subscribe {
                topic: topic.clone(),
            })
            .unwrap();
        let mut tries = 0;
        loop {
            let responses = client.poll_responses();
            if !responses.is_empty() {
                assert_eq!(responses[0].0, id);
                match &responses[0].1 {
                    RadioResponse::SubscribeAck { topic: t } => assert_eq!(t, &topic),
                    other => panic!("unexpected response: {other:?}"),
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            tries += 1;
            if tries > 1000 {
                panic!("no response within 1s");
            }
        }
    }

    #[test]
    fn multiple_requests_get_distinct_ids() {
        let client = RadioClient::spawn().unwrap();
        let ids: Vec<_> = (0..10)
            .map(|i| {
                client
                    .submit(RadioRequest::FetchParticle {
                        particle: ParticleId::from_entity(i, 0),
                    })
                    .unwrap()
            })
            .collect();

        let mut received = Vec::new();
        let mut tries = 0;
        while received.len() < 10 && tries < 1000 {
            received.extend(client.poll_responses());
            if received.len() < 10 {
                std::thread::sleep(std::time::Duration::from_millis(1));
                tries += 1;
            }
        }
        assert_eq!(received.len(), 10);
        let received_ids: std::collections::HashSet<_> =
            received.iter().map(|(id, _)| *id).collect();
        let expected_ids: std::collections::HashSet<_> = ids.iter().copied().collect();
        assert_eq!(received_ids, expected_ids);
    }

    #[test]
    fn client_drop_shuts_down_daemon() {
        let client = RadioClient::spawn().unwrap();
        let _ = client.submit(RadioRequest::FetchParticle {
            particle: ParticleId::from_entity(1, 0),
        });
        // Dropping the client closes request_tx; daemon exits cleanly.
        // No assertion needed — Drop completes without panic.
        drop(client);
    }

    #[test]
    fn poll_returns_empty_when_no_responses() {
        let client = RadioClient::spawn().unwrap();
        // Without submitting anything, poll returns empty immediately.
        assert!(client.poll_responses().is_empty());
        assert!(client.poll_gossip().is_empty());
    }
}
