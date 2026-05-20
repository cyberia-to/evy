//! Radio daemon — runs in its own thread. Session 11.1 ships a stub
//! daemon that echoes responses synchronously; session 11.2 replaces
//! the loop body with actual iroh calls inside a tokio runtime.

use std::sync::mpsc;
use std::thread;

use crate::client::{Envelope, RequestId};
use crate::message::{GossipEvent, RadioRequest, RadioResponse};
use evy_ecs_storage::ParticleId;

/// Handle to the daemon thread. Dropping it joins the thread cleanly
/// (the daemon exits when its inbound channel closes).
pub struct DaemonHandle {
    thread: Option<thread::JoinHandle<()>>,
}

impl DaemonHandle {
    /// Spawn the daemon thread.
    ///
    /// `request_rx` receives `Envelope` requests from the client.
    /// `response_tx` sends back `(RequestId, RadioResponse)` correlations.
    /// `gossip_tx` sends push-style gossip events (no RequestId).
    pub(crate) fn spawn(
        request_rx: mpsc::Receiver<Envelope>,
        response_tx: mpsc::Sender<(RequestId, RadioResponse)>,
        gossip_tx: mpsc::Sender<GossipEvent>,
    ) -> Self {
        let thread = thread::Builder::new()
            .name("evy-radio".into())
            .spawn(move || run_daemon(request_rx, response_tx, gossip_tx))
            .expect("spawn radio daemon thread");
        Self {
            thread: Some(thread),
        }
    }
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        // The client drops its sender first, which closes request_rx.
        // The daemon's loop exits on recv() == Err. Joining waits for
        // any in-flight work to complete.
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// The daemon loop. Session 11.1 stub: synchronous request handling
/// that fabricates responses for testing the bridge shape.
///
/// Session 11.2 replaces this with a tokio runtime that calls iroh's
/// async APIs (blob fetch, gossip publish/subscribe).
fn run_daemon(
    request_rx: mpsc::Receiver<Envelope>,
    response_tx: mpsc::Sender<(RequestId, RadioResponse)>,
    _gossip_tx: mpsc::Sender<GossipEvent>,
) {
    while let Ok(env) = request_rx.recv() {
        let Envelope { id, request } = env;
        let response = handle_request(request);
        if let Some(resp) = response {
            if response_tx.send((id, resp)).is_err() {
                // Client dropped its receiver; bail out.
                return;
            }
        } else {
            // Shutdown request — exit the loop.
            return;
        }
    }
}

/// Stub handler. Returns `None` for shutdown (signals loop exit),
/// otherwise a fabricated response.
fn handle_request(request: RadioRequest) -> Option<RadioResponse> {
    match request {
        RadioRequest::FetchParticle { particle } => Some(RadioResponse::Fetched {
            particle,
            // Stub: return zero bytes. Real impl fetches from iroh blob store.
            bytes: Vec::new(),
        }),
        RadioRequest::PublishParticle { bytes } => {
            // Stub: synthesize a deterministic ParticleId from the byte
            // length. Real impl hemera-hashes the bytes.
            let synthetic = ParticleId::from_entity(bytes.len() as u32, 0);
            Some(RadioResponse::Published {
                particle: synthetic,
            })
        }
        RadioRequest::Subscribe { topic } => Some(RadioResponse::SubscribeAck { topic }),
        RadioRequest::Unsubscribe { topic } => Some(RadioResponse::UnsubscribeAck { topic }),
        RadioRequest::GossipPublish { .. } => Some(RadioResponse::GossipPublishAck),
        RadioRequest::Shutdown => None,
    }
}
