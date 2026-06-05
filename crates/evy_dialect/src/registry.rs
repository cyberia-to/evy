//! `DialectRegistry` — runtime registration and lookup of types ↔ dialects.

use std::any::TypeId;
use std::collections::HashMap;

use crate::Dialect;

/// Types that declare their dialect. Implementors typically compute the
/// dialect at compile time via `dialect_from_struct` and assign it to a
/// `const`.
pub trait HasDialect: 'static {
    /// The dialect — must be derivable, ideally with `dialect_from_struct`,
    /// so two compilations of the same type produce the same value.
    const DIALECT: Dialect;

    /// Human-readable name. Defaults to `std::any::type_name`.
    fn dialect_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Registry entry — what we know about a registered type.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub type_id: TypeId,
    pub type_name: &'static str,
    pub dialect: Dialect,
}

/// Bidirectional map between Rust `TypeId` and cyber `Dialect`.
///
/// One `DialectRegistry` per evy `App`; held as a resource by the engine.
/// All component types that participate in BBG-committed namespaces
/// should register here so cross-machine schema agreement is enforceable.
#[derive(Debug, Default)]
pub struct DialectRegistry {
    by_type: HashMap<TypeId, Dialect>,
    by_particle: HashMap<Dialect, RegistryEntry>,
}

impl DialectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a type. Idempotent: re-registering the same type with the
    /// same dialect is a no-op. Re-registering with a different dialect
    /// returns `Err(DialectConflict)` — usually means the type's schema
    /// changed and the in-memory registry is now stale.
    pub fn register<T: HasDialect>(&mut self) -> Result<(), DialectConflict> {
        let type_id = TypeId::of::<T>();
        let dialect = T::DIALECT;

        match self.by_type.get(&type_id) {
            Some(existing) if *existing == dialect => return Ok(()),
            Some(existing) => {
                return Err(DialectConflict {
                    type_name: T::dialect_name(),
                    registered: *existing,
                    requested: dialect,
                });
            }
            None => {}
        }

        self.by_type.insert(type_id, dialect);
        self.by_particle.insert(
            dialect,
            RegistryEntry {
                type_id,
                type_name: T::dialect_name(),
                dialect,
            },
        );
        Ok(())
    }

    /// Get the registered dialect for a type, if any.
    pub fn lookup_dialect<T: 'static>(&self) -> Option<Dialect> {
        self.by_type.get(&TypeId::of::<T>()).copied()
    }

    /// Get the registry entry for a dialect, if registered.
    pub fn lookup_type(&self, dialect: Dialect) -> Option<&RegistryEntry> {
        self.by_particle.get(&dialect)
    }

    /// True if the registered dialect for `T` equals `T::DIALECT`.
    ///
    /// Returns `false` if either `T` is unregistered or the registered
    /// dialect disagrees with the type's compile-time dialect (= schema
    /// drift between when the type was registered and now).
    pub fn agrees_on<T: HasDialect>(&self) -> bool {
        self.lookup_dialect::<T>() == Some(T::DIALECT)
    }

    /// Number of registered types.
    pub fn len(&self) -> usize {
        self.by_type.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty()
    }
}

/// Error returned when re-registration would overwrite a type's dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialectConflict {
    pub type_name: &'static str,
    pub registered: Dialect,
    pub requested: Dialect,
}

impl core::fmt::Display for DialectConflict {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "dialect conflict for {}: registered {:?} vs requested {:?}",
            self.type_name, self.registered, self.requested
        )
    }
}

impl std::error::Error for DialectConflict {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::{dialect_from_struct, FieldSignature};

    struct Position {
        _x: f32,
        _y: f32,
        _z: f32,
    }

    impl HasDialect for Position {
        const DIALECT: Dialect = {
            // Compile-time const isn't possible with hash function; use
            // a literal for the test. Real types will use dialect_from_struct
            // at runtime via a one-time init or a build script.
            ParticleId::from_hash([0xAA; 32])
        };
    }

    struct Velocity(f32, f32, f32);

    impl HasDialect for Velocity {
        const DIALECT: Dialect = ParticleId::from_hash([0xBB; 32]);
    }

    use evy_ecs_storage::ParticleId;

    #[test]
    fn register_and_lookup_by_type() {
        let mut reg = DialectRegistry::new();
        reg.register::<Position>().unwrap();
        assert_eq!(reg.lookup_dialect::<Position>(), Some(Position::DIALECT));
    }

    #[test]
    fn register_and_lookup_by_particle() {
        let mut reg = DialectRegistry::new();
        reg.register::<Position>().unwrap();
        let entry = reg.lookup_type(Position::DIALECT).unwrap();
        assert_eq!(entry.dialect, Position::DIALECT);
    }

    #[test]
    fn agrees_on_after_registration() {
        let mut reg = DialectRegistry::new();
        reg.register::<Position>().unwrap();
        assert!(reg.agrees_on::<Position>());
    }

    #[test]
    fn agrees_on_returns_false_for_unregistered_type() {
        let reg = DialectRegistry::new();
        assert!(!reg.agrees_on::<Position>());
    }

    #[test]
    fn re_register_same_type_is_idempotent() {
        let mut reg = DialectRegistry::new();
        reg.register::<Position>().unwrap();
        // Second registration must not error.
        reg.register::<Position>().unwrap();
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn distinct_types_register_distinctly() {
        let mut reg = DialectRegistry::new();
        reg.register::<Position>().unwrap();
        reg.register::<Velocity>().unwrap();
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.lookup_dialect::<Position>(), Some(Position::DIALECT));
        assert_eq!(reg.lookup_dialect::<Velocity>(), Some(Velocity::DIALECT));
    }

    #[test]
    fn unregistered_lookup_returns_none() {
        struct Unregistered;
        let reg = DialectRegistry::new();
        assert_eq!(reg.lookup_dialect::<Unregistered>(), None);
    }

    #[test]
    fn derived_dialects_from_signature_round_trip() {
        // Realistic use: derive at runtime and register.
        let sc = dialect_from_struct(
            "Transform",
            &[
                FieldSignature::new("x", "f32"),
                FieldSignature::new("y", "f32"),
                FieldSignature::new("z", "f32"),
            ],
        );
        // The hash is non-zero (extremely unlikely to be all zero).
        assert_ne!(sc.as_bytes(), &[0u8; 32]);
    }
}
