//! The 13 evy storage namespaces — one wrapper enum over `bbg::storage::dim`.

use bbg::storage::dim;

/// Storage namespace for a component.
///
/// 10 public BBG_poly dimensions + 2 private polynomials + 1 ephemeral.
/// Authenticated namespaces (everything except [`Namespace::Ephemeral`])
/// participate in BBG_root and emit zheng proofs at tick boundaries.
/// Ephemeral writes never gossip to the network or contribute to BBG_root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Namespace {
    Particles = dim::PARTICLES,
    AxonsOut = dim::AXONS_OUT,
    AxonsIn = dim::AXONS_IN,
    Neurons = dim::NEURONS,
    Locations = dim::LOCATIONS,
    Coins = dim::COINS,
    Cards = dim::CARDS,
    Files = dim::FILES,
    Time = dim::TIME,
    Signals = dim::SIGNALS,
    /// A(x) — private commitment polynomial.
    Commitments = dim::COMMITMENTS,
    /// N(x) — private nullifier polynomial.
    Nullifiers = dim::NULLIFIERS,
    /// Local-only state. Skipped by `commit()`; never written to warm/cold tiers.
    Ephemeral = dim::EPHEMERAL,
}

impl Namespace {
    /// The bbg dimension byte for this namespace.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// True if writes to this namespace are committed into BBG_root.
    pub const fn is_authenticated(self) -> bool {
        !matches!(self, Namespace::Ephemeral)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_constants_align() {
        assert_eq!(Namespace::Particles.as_u8(), 0);
        assert_eq!(Namespace::Ephemeral.as_u8(), 12);
    }

    #[test]
    fn ephemeral_is_unauthenticated() {
        assert!(!Namespace::Ephemeral.is_authenticated());
        assert!(Namespace::Particles.is_authenticated());
        assert!(Namespace::Coins.is_authenticated());
    }
}
