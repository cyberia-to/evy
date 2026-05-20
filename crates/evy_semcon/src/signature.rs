//! Deterministic semcon derivation from a type signature.
//!
//! The signature is a canonical string representation of the type's
//! structure (name + ordered fields). Hashing it via [[hemera]] yields
//! a 32-byte particle that is deterministic across machines.
//!
//! Convention for struct signatures:
//!
//! `"struct <name>{<field_name>:<field_type>,<field_name>:<field_type>,...}"`
//!
//! Fields are in declaration order; whitespace is collapsed. Two
//! machines that compile the same Rust type produce the same signature
//! string and therefore the same semcon.

use crate::Semcon;
use evy_ecs_storage::ParticleId;

/// One field of a struct, for `semcon_from_struct`.
#[derive(Debug, Clone, Copy)]
pub struct FieldSignature {
    pub name: &'static str,
    pub type_name: &'static str,
}

impl FieldSignature {
    pub const fn new(name: &'static str, type_name: &'static str) -> Self {
        Self { name, type_name }
    }
}

/// Derive a semcon from an arbitrary signature string. Use this for
/// non-struct types or custom signatures.
pub fn semcon_from_signature(signature: &str) -> Semcon {
    let h = hemera::hash(signature.as_bytes());
    ParticleId::from_hash(*h.as_bytes())
}

/// Derive a semcon from a struct's name and ordered fields. Builds a
/// canonical string `"struct <name>{n1:t1,n2:t2,...}"` and hashes it.
pub fn semcon_from_struct(name: &str, fields: &[FieldSignature]) -> Semcon {
    let mut sig = String::with_capacity(name.len() + 16 + fields.len() * 16);
    sig.push_str("struct ");
    sig.push_str(name);
    sig.push('{');
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            sig.push(',');
        }
        sig.push_str(f.name);
        sig.push(':');
        sig.push_str(f.type_name);
    }
    sig.push('}');
    semcon_from_signature(&sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_signature_yields_same_semcon() {
        let a = semcon_from_signature("foo");
        let b = semcon_from_signature("foo");
        assert_eq!(a, b);
    }

    #[test]
    fn different_signatures_yield_different_semcons() {
        let a = semcon_from_signature("foo");
        let b = semcon_from_signature("bar");
        assert_ne!(a, b);
    }

    #[test]
    fn struct_semcon_is_field_order_sensitive() {
        let a = semcon_from_struct(
            "Point",
            &[FieldSignature::new("x", "f32"), FieldSignature::new("y", "f32")],
        );
        let b = semcon_from_struct(
            "Point",
            &[FieldSignature::new("y", "f32"), FieldSignature::new("x", "f32")],
        );
        // Different ordering = different schema = different semcon.
        assert_ne!(a, b);
    }

    #[test]
    fn struct_semcon_is_type_sensitive() {
        let a = semcon_from_struct(
            "Point",
            &[FieldSignature::new("x", "f32"), FieldSignature::new("y", "f32")],
        );
        let b = semcon_from_struct(
            "Point",
            &[FieldSignature::new("x", "f64"), FieldSignature::new("y", "f64")],
        );
        assert_ne!(a, b);
    }

    #[test]
    fn struct_semcon_is_name_sensitive() {
        let a = semcon_from_struct(
            "Point",
            &[FieldSignature::new("x", "f32"), FieldSignature::new("y", "f32")],
        );
        let b = semcon_from_struct(
            "Vec2",
            &[FieldSignature::new("x", "f32"), FieldSignature::new("y", "f32")],
        );
        assert_ne!(a, b);
    }

    #[test]
    fn empty_struct_has_distinct_semcon() {
        let a = semcon_from_struct("Marker", &[]);
        let b = semcon_from_struct("Marker", &[FieldSignature::new("v", "()")]);
        assert_ne!(a, b);
    }
}
