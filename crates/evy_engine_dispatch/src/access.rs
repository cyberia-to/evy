//! Dependency analysis on dispatch nodes' read/write sets.
//!
//! Two nodes conflict if any of:
//! - Both write to overlapping shard refs (write-write conflict)
//! - One writes, the other reads overlapping refs (read-write conflict)
//!
//! Reads alone never conflict. The dependency graph orders nodes such
//! that for every conflicting pair, the later one waits for the earlier.

use crate::node::ShardRef;

/// Computed access set for one node.
#[derive(Debug, Clone, Default)]
pub struct AccessSet {
    pub reads: Vec<ShardRef>,
    pub writes: Vec<ShardRef>,
}

impl AccessSet {
    /// Build from a node's `reads()` + `writes()` results.
    pub fn from_node_refs(reads: &[ShardRef], writes: &[ShardRef]) -> Self {
        Self {
            reads: reads.to_vec(),
            writes: writes.to_vec(),
        }
    }

    /// True if this access set conflicts with `other` — they cannot run in
    /// parallel and must be ordered by the scheduler.
    pub fn conflicts_with(&self, other: &Self) -> bool {
        // write-write: any pair of writes that aren't disjoint
        for w_self in &self.writes {
            for w_other in &other.writes {
                if !w_self.disjoint(*w_other) {
                    return true;
                }
            }
        }
        // read-write: my reads vs other's writes
        for r_self in &self.reads {
            for w_other in &other.writes {
                if !r_self.disjoint(*w_other) {
                    return true;
                }
            }
        }
        // write-read: my writes vs other's reads
        for w_self in &self.writes {
            for r_other in &other.reads {
                if !w_self.disjoint(*r_other) {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evy_ecs_storage::{Namespace, ParticleId};

    fn ns(n: Namespace) -> ShardRef {
        ShardRef::Namespace(n)
    }
    fn pa(n: Namespace, idx: u32) -> ShardRef {
        ShardRef::Particle(n, ParticleId::from_entity(idx, 0))
    }

    #[test]
    fn read_only_pairs_dont_conflict() {
        let a = AccessSet {
            reads: vec![ns(Namespace::Coins)],
            writes: vec![],
        };
        let b = AccessSet {
            reads: vec![ns(Namespace::Coins)],
            writes: vec![],
        };
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn write_write_same_namespace_conflicts() {
        let a = AccessSet {
            reads: vec![],
            writes: vec![ns(Namespace::Coins)],
        };
        let b = AccessSet {
            reads: vec![],
            writes: vec![ns(Namespace::Coins)],
        };
        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn write_write_different_namespaces_dont_conflict() {
        let a = AccessSet {
            reads: vec![],
            writes: vec![ns(Namespace::Coins)],
        };
        let b = AccessSet {
            reads: vec![],
            writes: vec![ns(Namespace::Cards)],
        };
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn read_write_overlap_conflicts() {
        let reader = AccessSet {
            reads: vec![ns(Namespace::Coins)],
            writes: vec![],
        };
        let writer = AccessSet {
            reads: vec![],
            writes: vec![ns(Namespace::Coins)],
        };
        assert!(reader.conflicts_with(&writer));
        assert!(writer.conflicts_with(&reader));
    }

    #[test]
    fn particle_writes_on_same_namespace_different_particles_dont_conflict() {
        let a = AccessSet {
            reads: vec![],
            writes: vec![pa(Namespace::Particles, 1)],
        };
        let b = AccessSet {
            reads: vec![],
            writes: vec![pa(Namespace::Particles, 2)],
        };
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn namespace_write_blocks_any_particle_in_namespace() {
        let coarse = AccessSet {
            reads: vec![],
            writes: vec![ns(Namespace::Particles)],
        };
        let fine = AccessSet {
            reads: vec![pa(Namespace::Particles, 5)],
            writes: vec![],
        };
        assert!(coarse.conflicts_with(&fine));
    }
}
