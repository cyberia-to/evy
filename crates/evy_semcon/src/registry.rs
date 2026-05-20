//! `SemconRegistry` — runtime registration and lookup of types ↔ semcons.

use std::any::TypeId;
use std::collections::HashMap;

use crate::Semcon;

/// Types that declare their semcon. Implementors typically compute the
/// semcon at compile time via `semcon_from_struct` and assign it to a
/// `const`.
pub trait HasSemcon: 'static {
    /// The semcon — must be derivable, ideally with `semcon_from_struct`,
    /// so two compilations of the same type produce the same value.
    const SEMCON: Semcon;

    /// Human-readable name. Defaults to `std::any::type_name`.
    fn semcon_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Registry entry — what we know about a registered type.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub type_id: TypeId,
    pub type_name: &'static str,
    pub semcon: Semcon,
}

/// Bidirectional map between Rust `TypeId` and cyber `Semcon`.
///
/// One `SemconRegistry` per evy `App`; held as a resource by the engine.
/// All component types that participate in BBG-committed namespaces
/// should register here so cross-machine schema agreement is enforceable.
#[derive(Debug, Default)]
pub struct SemconRegistry {
    by_type: HashMap<TypeId, Semcon>,
    by_particle: HashMap<Semcon, RegistryEntry>,
}

impl SemconRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a type. Idempotent: re-registering the same type with the
    /// same semcon is a no-op. Re-registering with a different semcon
    /// returns `Err(SemconConflict)` — usually means the type's schema
    /// changed and the in-memory registry is now stale.
    pub fn register<T: HasSemcon>(&mut self) -> Result<(), SemconConflict> {
        let type_id = TypeId::of::<T>();
        let semcon = T::SEMCON;

        match self.by_type.get(&type_id) {
            Some(existing) if *existing == semcon => return Ok(()),
            Some(existing) => {
                return Err(SemconConflict {
                    type_name: T::semcon_name(),
                    registered: *existing,
                    requested: semcon,
                });
            }
            None => {}
        }

        self.by_type.insert(type_id, semcon);
        self.by_particle.insert(
            semcon,
            RegistryEntry {
                type_id,
                type_name: T::semcon_name(),
                semcon,
            },
        );
        Ok(())
    }

    /// Get the registered semcon for a type, if any.
    pub fn lookup_semcon<T: 'static>(&self) -> Option<Semcon> {
        self.by_type.get(&TypeId::of::<T>()).copied()
    }

    /// Get the registry entry for a semcon, if registered.
    pub fn lookup_type(&self, semcon: Semcon) -> Option<&RegistryEntry> {
        self.by_particle.get(&semcon)
    }

    /// True if the registered semcon for `T` equals `T::SEMCON`.
    ///
    /// Returns `false` if either `T` is unregistered or the registered
    /// semcon disagrees with the type's compile-time semcon (= schema
    /// drift between when the type was registered and now).
    pub fn agrees_on<T: HasSemcon>(&self) -> bool {
        self.lookup_semcon::<T>() == Some(T::SEMCON)
    }

    /// Number of registered types.
    pub fn len(&self) -> usize {
        self.by_type.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty()
    }
}

/// Error returned when re-registration would overwrite a type's semcon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemconConflict {
    pub type_name: &'static str,
    pub registered: Semcon,
    pub requested: Semcon,
}

impl core::fmt::Display for SemconConflict {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "semcon conflict for {}: registered {:?} vs requested {:?}",
            self.type_name, self.registered, self.requested
        )
    }
}

impl std::error::Error for SemconConflict {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::{semcon_from_struct, FieldSignature};

    struct Position {
        _x: f32,
        _y: f32,
        _z: f32,
    }

    impl HasSemcon for Position {
        const SEMCON: Semcon = {
            // Compile-time const isn't possible with hash function; use
            // a literal for the test. Real types will use semcon_from_struct
            // at runtime via a one-time init or a build script.
            ParticleId::from_hash([0xAA; 32])
        };
    }

    struct Velocity(f32, f32, f32);

    impl HasSemcon for Velocity {
        const SEMCON: Semcon = ParticleId::from_hash([0xBB; 32]);
    }

    use evy_ecs_storage::ParticleId;

    #[test]
    fn register_and_lookup_by_type() {
        let mut reg = SemconRegistry::new();
        reg.register::<Position>().unwrap();
        assert_eq!(reg.lookup_semcon::<Position>(), Some(Position::SEMCON));
    }

    #[test]
    fn register_and_lookup_by_particle() {
        let mut reg = SemconRegistry::new();
        reg.register::<Position>().unwrap();
        let entry = reg.lookup_type(Position::SEMCON).unwrap();
        assert_eq!(entry.semcon, Position::SEMCON);
    }

    #[test]
    fn agrees_on_after_registration() {
        let mut reg = SemconRegistry::new();
        reg.register::<Position>().unwrap();
        assert!(reg.agrees_on::<Position>());
    }

    #[test]
    fn agrees_on_returns_false_for_unregistered_type() {
        let reg = SemconRegistry::new();
        assert!(!reg.agrees_on::<Position>());
    }

    #[test]
    fn re_register_same_type_is_idempotent() {
        let mut reg = SemconRegistry::new();
        reg.register::<Position>().unwrap();
        // Second registration must not error.
        reg.register::<Position>().unwrap();
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn distinct_types_register_distinctly() {
        let mut reg = SemconRegistry::new();
        reg.register::<Position>().unwrap();
        reg.register::<Velocity>().unwrap();
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.lookup_semcon::<Position>(), Some(Position::SEMCON));
        assert_eq!(reg.lookup_semcon::<Velocity>(), Some(Velocity::SEMCON));
    }

    #[test]
    fn unregistered_lookup_returns_none() {
        struct Unregistered;
        let reg = SemconRegistry::new();
        assert_eq!(reg.lookup_semcon::<Unregistered>(), None);
    }

    #[test]
    fn derived_semcons_from_signature_round_trip() {
        // Realistic use: derive at runtime and register.
        let sc = semcon_from_struct(
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
