//! ParticleId — 32-byte content-addressed or generation-tagged key into ShardStore.
//!
//! Two construction paths:
//! - `from_entity(index, gen)` — local-only ephemeral entity. First 8 bytes
//!   pack the Bevy-like (index, generation) pair; remaining 24 bytes are zero.
//!   Generation bump on respawn changes the key, so stale GPU descriptors
//!   targeting a freed slot fail validation on read.
//! - `from_hash([u8; 32])` — content-addressed particle. The key IS the
//!   hemera hash of content. Bevy entities for these are deterministic
//!   functions of the hash, so `from_hash(h)` on two machines yields the
//!   same ParticleId.

use core::fmt;

/// 32-byte storage key. Same width as a hemera particle hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParticleId(pub [u8; 32]);

impl ParticleId {
    /// Construct from a Bevy-like (entity_index, generation) pair. Used for
    /// local-only entities (Namespace::Ephemeral, render scratch, etc.) where
    /// no content hash is meaningful.
    pub const fn from_entity(index: u32, generation: u32) -> Self {
        let mut k = [0u8; 32];
        let idx_bytes = index.to_le_bytes();
        let gen_bytes = generation.to_le_bytes();
        k[0] = idx_bytes[0];
        k[1] = idx_bytes[1];
        k[2] = idx_bytes[2];
        k[3] = idx_bytes[3];
        k[4] = gen_bytes[0];
        k[5] = gen_bytes[1];
        k[6] = gen_bytes[2];
        k[7] = gen_bytes[3];
        Self(k)
    }

    /// Construct from a 32-byte particle hash (hemera output).
    pub const fn from_hash(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Raw key bytes for ShardStore access.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Owned key bytes (for `put` which takes by value).
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Extract (index, generation) if this ParticleId was constructed from an
    /// entity pair. For hash-constructed particles, returns whatever bytes
    /// happen to occupy that region — caller's responsibility to track which
    /// construction path was used.
    pub fn entity_parts(&self) -> (u32, u32) {
        let mut idx = [0u8; 4];
        let mut g = [0u8; 4];
        idx.copy_from_slice(&self.0[0..4]);
        g.copy_from_slice(&self.0[4..8]);
        (u32::from_le_bytes(idx), u32::from_le_bytes(g))
    }
}

impl fmt::Debug for ParticleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // First 4 + last 4 hex bytes — enough to disambiguate, short enough to read.
        write!(
            f,
            "ParticleId({:02x}{:02x}{:02x}{:02x}…{:02x}{:02x}{:02x}{:02x})",
            self.0[0], self.0[1], self.0[2], self.0[3],
            self.0[28], self.0[29], self.0[30], self.0[31],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_round_trip() {
        let p = ParticleId::from_entity(42, 7);
        assert_eq!(p.entity_parts(), (42, 7));
    }

    #[test]
    fn generation_bump_changes_key() {
        let a = ParticleId::from_entity(42, 7);
        let b = ParticleId::from_entity(42, 8);
        assert_ne!(a, b);
    }

    #[test]
    fn hash_construction_preserves_bytes() {
        let h = [0xab; 32];
        let p = ParticleId::from_hash(h);
        assert_eq!(p.as_bytes(), &h);
    }
}
