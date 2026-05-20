//! EvyComponent — typed components that know their namespace and encoding.
//!
//! For session 1 the trait is manually implemented. A `#[derive(EvyComponent)]`
//! procedural macro lands later (spec §19 step 4 or thereabouts) and will
//! synthesize the encode/decode pair from the field types' `GoldilocksCodec`
//! impls.

use crate::namespace::Namespace;
use nebu::Goldilocks;

/// A typed component that lives in evy's ShardStore-backed storage.
///
/// Each impl declares one storage namespace and provides field-element
/// serialization. The runtime backend (`memory`/`unimem`/`ssd`/`hdd`) is
/// chosen by `ShardStorage` from `PlatformCapabilities`; the component
/// definition is backend-agnostic.
pub trait EvyComponent: Sized {
    /// Storage namespace. Determines whether writes are committed into
    /// BBG_root (authenticated namespaces) or kept local-only (Ephemeral).
    const NAMESPACE: Namespace;

    /// Encode this value to a sequence of field elements.
    fn to_goldilocks(&self) -> Vec<Goldilocks>;

    /// Decode from a slice of field elements. The slice length is whatever
    /// `to_goldilocks` produced for this type.
    fn from_goldilocks(slice: &[Goldilocks]) -> Self;
}

/// Blanket impl: any type that has a `GoldilocksCodec` and an associated
/// `NAMESPACE` const can be used as an `EvyComponent` via a tiny wrapper.
/// Most callers will implement `EvyComponent` directly on their components.
///
/// (No blanket impl is provided directly because Rust can't synthesize the
/// per-type `NAMESPACE` const without a derive macro.)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::GoldilocksCodec;

    /// Example component: a simple 3D position.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
        z: f32,
    }

    impl EvyComponent for Position {
        const NAMESPACE: Namespace = Namespace::Ephemeral;

        fn to_goldilocks(&self) -> Vec<Goldilocks> {
            let mut v = Vec::with_capacity(3);
            self.x.encode_into(&mut v);
            self.y.encode_into(&mut v);
            self.z.encode_into(&mut v);
            v
        }

        fn from_goldilocks(slice: &[Goldilocks]) -> Self {
            Self {
                x: f32::decode_from(&slice[0..1]),
                y: f32::decode_from(&slice[1..2]),
                z: f32::decode_from(&slice[2..3]),
            }
        }
    }

    /// Example component in the coins namespace (BBG-committed).
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Balance(u64);

    impl EvyComponent for Balance {
        const NAMESPACE: Namespace = Namespace::Coins;

        fn to_goldilocks(&self) -> Vec<Goldilocks> {
            // Balance values are gameplay-bounded; we ensure they fit
            // Goldilocks elsewhere (caps at protocol level).
            self.0.encode()
        }

        fn from_goldilocks(slice: &[Goldilocks]) -> Self {
            Self(u64::decode_from(slice))
        }
    }

    #[test]
    fn position_round_trip() {
        let p = Position {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let enc = p.to_goldilocks();
        assert_eq!(enc.len(), 3);
        let dec = Position::from_goldilocks(&enc);
        assert_eq!(dec, p);
    }

    #[test]
    fn balance_round_trip() {
        let b = Balance(1_000_000);
        let enc = b.to_goldilocks();
        assert_eq!(enc.len(), 1);
        let dec = Balance::from_goldilocks(&enc);
        assert_eq!(dec, b);
    }

    #[test]
    fn namespaces_match() {
        assert_eq!(Position::NAMESPACE, Namespace::Ephemeral);
        assert!(!Position::NAMESPACE.is_authenticated());
        assert_eq!(Balance::NAMESPACE, Namespace::Coins);
        assert!(Balance::NAMESPACE.is_authenticated());
    }
}
