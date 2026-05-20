//! `DispatchNode` — the unified node trait. One shape covers render nodes,
//! gameplay signal handlers, and compute kernels.

use evy_ecs_storage::{Namespace, ParticleId};
use evy_platform_caps::Engine;

use crate::ctx::DispatchCtx;

/// A reference to a region of evy state — used by [`DispatchNode::reads`]
/// and [`DispatchNode::writes`] to declare access for dependency analysis.
///
/// Namespace-level refs are the common case (a node touches "all transforms"
/// or "all currency balances"). Particle-level refs let a node declare
/// scoped access ("only this particle's neural feature map") for finer
/// parallelism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShardRef {
    /// All entries within a namespace.
    Namespace(Namespace),
    /// A specific particle's slot within a namespace.
    Particle(Namespace, ParticleId),
}

impl ShardRef {
    /// True if two refs definitely don't conflict (read/write on disjoint state).
    /// A particle-level ref into namespace N is disjoint from a particle-level
    /// ref into namespace N at a different particle. Anything else is treated
    /// as potentially conflicting for the conservative answer.
    pub fn disjoint(self, other: Self) -> bool {
        match (self, other) {
            (ShardRef::Particle(a_ns, a_p), ShardRef::Particle(b_ns, b_p)) => {
                a_ns != b_ns || a_p != b_p
            }
            (ShardRef::Namespace(a), ShardRef::Namespace(b)) => a != b,
            (ShardRef::Namespace(ns), ShardRef::Particle(p_ns, _))
            | (ShardRef::Particle(p_ns, _), ShardRef::Namespace(ns)) => ns != p_ns,
        }
    }
}

/// What the scheduler does with a node's writes at the tick boundary.
///
/// Per `evy/specs/evy.md` §6.1: render nodes are the `None` subset of the
/// dispatch DAG; gameplay nodes use `BbgDimension` or `BbgPrivate` to
/// trigger BBG commit + zheng proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitPolicy {
    /// Ephemeral writes. No commit, no proof, no replication.
    /// Render output, scratch compute, frame-local state.
    None,
    /// Writes update a BBG_poly dimension. Recommitted at tick boundary,
    /// emit zheng proof, gossiped via radio. The Namespace argument names
    /// which dimension this node writes to.
    BbgDimension(Namespace),
    /// Writes extend the A(x) commitment polynomial. Used for individual
    /// cyberlink records (private state).
    BbgPrivate,
}

impl CommitPolicy {
    /// True if this policy causes any tick-boundary commit work.
    pub fn is_committed(self) -> bool {
        !matches!(self, CommitPolicy::None)
    }
}

/// A unit of dispatchable work in the evy engine DAG.
///
/// Implementors declare their preferred engine, their read/write access
/// set on the shared `ShardStorage`, and a commit policy. The scheduler
/// resolves the engine against `PlatformCapabilities`, builds a dependency
/// graph from read/write sets, topo-sorts, and calls `dispatch` on each
/// node in turn.
///
/// `Send` because the scheduler may move nodes to engine-specific worker
/// pools.
pub trait DispatchNode: Send {
    /// Preferred engine for this node's work.
    fn engine(&self) -> Engine;

    /// Shards this node reads. Used for dependency analysis.
    fn reads(&self) -> &[ShardRef];

    /// Shards this node writes. Used for dependency analysis.
    fn writes(&self) -> &[ShardRef];

    /// What to do with writes at the tick boundary.
    fn commit_policy(&self) -> CommitPolicy;

    /// Execute the node. Called by the scheduler in topo order.
    fn dispatch(&mut self, ctx: &mut DispatchCtx<'_>);

    /// Human-readable name for debugging and dispatch traces.
    fn label(&self) -> &str {
        core::any::type_name::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_refs_disjoint_when_different() {
        assert!(ShardRef::Namespace(Namespace::Particles)
            .disjoint(ShardRef::Namespace(Namespace::Coins)));
        assert!(!ShardRef::Namespace(Namespace::Particles)
            .disjoint(ShardRef::Namespace(Namespace::Particles)));
    }

    #[test]
    fn particle_refs_disjoint_when_different_particle() {
        let a = ParticleId::from_entity(1, 0);
        let b = ParticleId::from_entity(2, 0);
        assert!(ShardRef::Particle(Namespace::Neurons, a)
            .disjoint(ShardRef::Particle(Namespace::Neurons, b)));
        assert!(!ShardRef::Particle(Namespace::Neurons, a)
            .disjoint(ShardRef::Particle(Namespace::Neurons, a)));
    }

    #[test]
    fn namespace_and_particle_refs_conflict_in_same_namespace() {
        let p = ParticleId::from_entity(1, 0);
        assert!(!ShardRef::Namespace(Namespace::Coins)
            .disjoint(ShardRef::Particle(Namespace::Coins, p)));
        assert!(ShardRef::Namespace(Namespace::Coins)
            .disjoint(ShardRef::Particle(Namespace::Cards, p)));
    }

    #[test]
    fn commit_policy_classification() {
        assert!(!CommitPolicy::None.is_committed());
        assert!(CommitPolicy::BbgDimension(Namespace::Coins).is_committed());
        assert!(CommitPolicy::BbgPrivate.is_committed());
    }
}
