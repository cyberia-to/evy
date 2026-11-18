//! `ShardStorage` — the bbg::ShardStore wrapper that gives typed component access.
//!
//! Owns one `Box<dyn ShardStore>`. The concrete backend is selected at
//! construction from `PlatformCapabilities`:
//!
//! | platform | backend |
//! |----------|---------|
//! | Apple Silicon with `backend-unimem` | `UnimemStore` (IOSurface-pinned) |
//! | everything else | `MemStore` (HashMap-backed) |
//!
//! `TieredStore` composition (memory hot + ssd warm + hdd cold) is a later
//! enhancement — for session 1 we keep a single hot backend.

use bbg::storage::{MemStore, ScanLimits, ShardStore, StorageResult};
use evy_platform_caps::PlatformCapabilities;
use nebu::Goldilocks;

use crate::{component::EvyComponent, particle::ParticleId};

/// Errors returnable by `reserve_pool`. The unimem-only operation can fail
/// if either the feature isn't enabled or IOSurface allocation fails.
#[derive(Debug)]
pub enum ReservePoolError {
    /// Pool allocation requires the `unimem` backend (Apple Silicon + feature flag).
    BackendUnsupported,
    /// IOSurface allocation failed (out of memory, unsupported size, etc.).
    AllocationFailed(String),
}

impl core::fmt::Display for ReservePoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BackendUnsupported => {
                write!(f, "reserve_pool requires the unimem backend")
            }
            Self::AllocationFailed(msg) => write!(f, "IOSurface allocation failed: {msg}"),
        }
    }
}

impl std::error::Error for ReservePoolError {}

/// Typed component storage over bbg's `ShardStore`.
///
/// Constructed once per evy `App`; held as a resource. Every component
/// access — `put`, `get`, `iter`, `remove` — routes through the underlying
/// `ShardStore` impl.
pub struct ShardStorage {
    inner: Box<dyn ShardStore>,
}

impl ShardStorage {
    /// Build a `ShardStorage` with the backend that best fits the host
    /// platform per `caps`. Apple Silicon + `backend-unimem` → `UnimemStore`;
    /// otherwise → `MemStore`.
    pub fn from_capabilities(caps: &PlatformCapabilities) -> Self {
        let inner = pick_backend(caps);
        Self { inner }
    }

    /// Build with an explicit backend. Useful for tests, custom topologies
    /// (TieredStore), or platforms whose caps probe is incomplete.
    pub fn with_backend(inner: Box<dyn ShardStore>) -> Self {
        Self { inner }
    }

    /// Insert a typed component for a particle. Replaces any existing value
    /// at the same `(namespace, particle)` slot.
    pub fn put<T: EvyComponent>(&mut self, particle: ParticleId, value: &T) -> StorageResult<()> {
        let encoded = value.to_goldilocks();
        self.inner
            .put(T::NAMESPACE.as_u8(), particle.to_bytes(), encoded)
    }

    /// Read a typed component from the cache. Use `read` for persisted values.
    pub fn get<T: EvyComponent>(&self, particle: ParticleId) -> Option<T> {
        let slice = self.inner.get(T::NAMESPACE.as_u8(), particle.as_bytes())?;
        Some(T::from_goldilocks(slice))
    }

    /// Read the raw field-element slice without decoding. For zero-copy
    /// shader-visible reads where the consumer can interpret raw u64 words.
    pub fn get_raw<T: EvyComponent>(&self, particle: ParticleId) -> Option<&[Goldilocks]> {
        self.inner.get(T::NAMESPACE.as_u8(), particle.as_bytes())
    }

    /// Read an owned component, including persisted values, within a field-element budget.
    pub fn read<T: EvyComponent>(
        &self,
        particle: ParticleId,
        max_elements: usize,
    ) -> StorageResult<Option<T>> {
        Ok(self.inner.read(T::NAMESPACE.as_u8(), particle.as_bytes(), max_elements)?
            .map(|value| T::from_goldilocks(&value)))
    }

    /// Read an ascending, bounded page after `after`. Disk backends require
    /// pending writes to be committed first. Propagates scan and decoding errors
    /// from BBG; the component's own decoder retains its existing contract.
    pub fn scan<T: EvyComponent>(
        &self,
        after: Option<ParticleId>,
        limits: ScanLimits,
    ) -> StorageResult<Vec<(ParticleId, T)>> {
        Ok(self.inner.scan(T::NAMESPACE.as_u8(), after.map(|p| p.to_bytes()), limits)?
            .into_iter()
            .map(|(key, value)| (ParticleId(key), T::from_goldilocks(&value)))
            .collect())
    }

    /// Mutable raw slice for in-place updates. Caller must call `mark_dirty`
    /// after writing (unless the namespace is `Ephemeral`). Returns `None`
    /// on disk backends (fjall, redb) or if the entry doesn't exist.
    pub fn get_mut_raw<T: EvyComponent>(
        &mut self,
        particle: ParticleId,
    ) -> Option<&mut [Goldilocks]> {
        self.inner
            .get_mut(T::NAMESPACE.as_u8(), particle.as_bytes())
    }

    /// Mark a previously-mutated entry dirty for the next `commit()`.
    /// No-op for `Ephemeral` and on disk backends.
    pub fn mark_dirty<T: EvyComponent>(&mut self, particle: ParticleId) -> StorageResult<()> {
        self.inner
            .mark_dirty(T::NAMESPACE.as_u8(), particle.to_bytes())
    }

    /// Remove a typed component. Returns the previous value if present.
    pub fn remove<T: EvyComponent>(&mut self, particle: ParticleId) -> StorageResult<Option<T>> {
        let raw = self
            .inner
            .remove(T::NAMESPACE.as_u8(), particle.as_bytes())?;
        Ok(raw.map(|value| T::from_goldilocks(&value)))
    }

    /// Iterate cached `(ParticleId, T)` pairs in this component's namespace.
    /// Order is backend-dependent. Use `scan` for bounded persisted reads.
    pub fn iter<T: EvyComponent>(&self) -> impl Iterator<Item = (ParticleId, T)> + '_ {
        self.inner.iter(T::NAMESPACE.as_u8()).map(|(k, v)| {
            // Iterator yields refs whose lifetimes are bound to the boxed
            // iterator; we decode eagerly (T owns the result), so the iterator
            // outlives the references it touched.
            (ParticleId(*k), T::from_goldilocks(v))
        })
    }

    /// Commit pending entries and return the backend's change identity.
    /// Errors, including an unknown commit outcome, reach the caller unchanged.
    /// BBG computes authenticated state roots independently of this identity.
    pub fn commit(&mut self) -> StorageResult<[u8; 32]> {
        self.inner.commit()
    }

    /// Pre-allocate a slot pool for a component type. Subsequent `put`s into
    /// this namespace allocate cells from one contiguous IOSurface Block
    /// instead of per-entry Blocks — stable addresses for GPU/ANE descriptor
    /// binding.
    ///
    /// Apple Silicon + `backend-unimem` only. On other backends returns
    /// `ReservePoolError::BackendUnsupported`.
    #[cfg(all(target_os = "macos", feature = "backend-unimem"))]
    pub fn reserve_pool<T: EvyComponent>(
        &mut self,
        expected_count: usize,
    ) -> Result<(), ReservePoolError> {
        use bbg::storage::UnimemStore;
        // Downcast through Any. ShardStore: Send+Sync; we need a concrete
        // type erasure that exposes the pool API. UnimemStore implements
        // ShardStore — the only way to reach reserve_pool from here is to
        // downcast, which the trait doesn't currently support.
        //
        // For session 1 we punt: `reserve_pool` is exposed on UnimemStore
        // directly. Users who want it call `ShardStorage::with_backend`
        // with a pre-configured UnimemStore. Generic-shaped reserve_pool
        // through the ShardStore trait is a future extension (would require
        // either Any downcast or a new trait method).
        let _ = expected_count;
        Err(ReservePoolError::BackendUnsupported)
    }

    #[cfg(not(all(target_os = "macos", feature = "backend-unimem")))]
    pub fn reserve_pool<T: EvyComponent>(
        &mut self,
        _expected_count: usize,
    ) -> Result<(), ReservePoolError> {
        Err(ReservePoolError::BackendUnsupported)
    }
}

/// Pick a backend appropriate to the current platform. On Apple Silicon with
/// the `backend-unimem` feature, prefer `UnimemStore` for zero-copy
/// CPU/AMX/GPU/ANE access. Everything else falls back to `MemStore`.
fn pick_backend(_caps: &PlatformCapabilities) -> Box<dyn ShardStore> {
    #[cfg(all(target_os = "macos", feature = "backend-unimem"))]
    {
        if _caps.has_unimem {
            return Box::new(bbg::storage::UnimemStore::new());
        }
    }
    Box::new(MemStore::new())
}

#[cfg(test)]
#[path = "storage_failures.rs"]
mod failures;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::Namespace;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
        z: f32,
    }

    impl EvyComponent for Position {
        const NAMESPACE: Namespace = Namespace::Ephemeral;
        fn to_goldilocks(&self) -> Vec<Goldilocks> {
            use crate::codec::GoldilocksCodec;
            let mut v = Vec::with_capacity(3);
            self.x.encode_into(&mut v);
            self.y.encode_into(&mut v);
            self.z.encode_into(&mut v);
            v
        }
        fn from_goldilocks(slice: &[Goldilocks]) -> Self {
            use crate::codec::GoldilocksCodec;
            Self {
                x: f32::decode_from(&slice[0..1]),
                y: f32::decode_from(&slice[1..2]),
                z: f32::decode_from(&slice[2..3]),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Balance(u64);

    impl EvyComponent for Balance {
        const NAMESPACE: Namespace = Namespace::Coins;
        fn to_goldilocks(&self) -> Vec<Goldilocks> {
            use crate::codec::GoldilocksCodec;
            self.0.encode()
        }
        fn from_goldilocks(slice: &[Goldilocks]) -> Self {
            use crate::codec::GoldilocksCodec;
            Self(u64::decode_from(slice))
        }
    }

    fn fresh_storage() -> ShardStorage {
        ShardStorage::with_backend(Box::new(MemStore::new()))
    }

    #[test]
    fn put_get_round_trip_ephemeral() {
        let mut s = fresh_storage();
        let p = ParticleId::from_entity(1, 0);
        let v = Position { x: 1.0, y: 2.0, z: 3.0 };
        s.put(p, &v).unwrap();
        assert_eq!(s.get::<Position>(p), Some(v));
    }

    #[test]
    fn put_get_round_trip_authenticated() {
        let mut s = fresh_storage();
        let p = ParticleId::from_entity(2, 0);
        let b = Balance(1_000_000);
        s.put(p, &b).unwrap();
        assert_eq!(s.get::<Balance>(p), Some(b));
    }

    #[test]
    fn ephemeral_writes_skip_commit_root() {
        let mut s = fresh_storage();
        let p = ParticleId::from_entity(3, 0);
        s.put(p, &Position { x: 0.0, y: 0.0, z: 0.0 }).unwrap();
        // ephemeral writes don't appear in dirty; commit hashes an empty list.
        let root_a = s.commit().unwrap();
        let root_b = s.commit().unwrap();
        assert_eq!(root_a, root_b, "no-op commits must be deterministic");
    }

    #[test]
    fn authenticated_writes_change_root() {
        let mut s = fresh_storage();
        let p1 = ParticleId::from_entity(4, 0);
        let p2 = ParticleId::from_entity(5, 0);
        s.put(p1, &Balance(100)).unwrap();
        let root_a = s.commit().unwrap();
        s.put(p2, &Balance(200)).unwrap();
        let root_b = s.commit().unwrap();
        assert_ne!(root_a, root_b, "authenticated puts must change shard root");
    }

    #[test]
    fn missing_get_returns_none() {
        let s = fresh_storage();
        let p = ParticleId::from_entity(99, 0);
        assert_eq!(s.get::<Position>(p), None);
        assert_eq!(s.get::<Balance>(p), None);
    }

    #[test]
    fn remove_returns_previous_value() {
        let mut s = fresh_storage();
        let p = ParticleId::from_entity(6, 0);
        let v = Position { x: 4.0, y: 5.0, z: 6.0 };
        s.put(p, &v).unwrap();
        assert_eq!(s.remove::<Position>(p).unwrap(), Some(v));
        assert_eq!(s.get::<Position>(p), None);
    }

    #[test]
    fn generation_isolates_slots() {
        let mut s = fresh_storage();
        let a = ParticleId::from_entity(7, 0);
        let b = ParticleId::from_entity(7, 1); // same index, bumped gen
        s.put(a, &Balance(111)).unwrap();
        s.put(b, &Balance(222)).unwrap();
        assert_eq!(s.get::<Balance>(a), Some(Balance(111)));
        assert_eq!(s.get::<Balance>(b), Some(Balance(222)));
    }

    #[test]
    fn iter_visits_only_one_namespace() {
        let mut s = fresh_storage();
        s.put(ParticleId::from_entity(10, 0), &Position { x: 1.0, y: 0.0, z: 0.0 }).unwrap();
        s.put(ParticleId::from_entity(11, 0), &Position { x: 2.0, y: 0.0, z: 0.0 }).unwrap();
        s.put(ParticleId::from_entity(12, 0), &Balance(42)).unwrap();

        let positions: Vec<_> = s.iter::<Position>().collect();
        assert_eq!(positions.len(), 2);
        let balances: Vec<_> = s.iter::<Balance>().collect();
        assert_eq!(balances.len(), 1);
        assert_eq!(balances[0].1, Balance(42));
    }

    #[test]
    fn get_mut_and_mark_dirty_path() {
        let mut s = fresh_storage();
        let p = ParticleId::from_entity(20, 0);
        s.put(p, &Balance(10)).unwrap();
        s.commit().unwrap(); // clear dirty after initial put

        {
            let slice = s.get_mut_raw::<Balance>(p).expect("mem backend supports get_mut");
            // Bypass codec for low-level update.
            slice[0] = Goldilocks::new(999);
        }
        s.mark_dirty::<Balance>(p).unwrap();

        // After mark_dirty, commit changes the root.
        let root_a = s.commit().unwrap();
        let root_b = s.commit().unwrap();
        assert_ne!(root_a, root_b, "mark_dirty after get_mut must affect commit");
        assert_eq!(s.get::<Balance>(p), Some(Balance(999)));
    }

    #[test]
    fn auto_select_backend_works() {
        let caps = evy_platform_caps::PlatformCapabilities::probe();
        let mut s = ShardStorage::from_capabilities(&caps);
        let p = ParticleId::from_entity(42, 0);
        s.put(p, &Position { x: 1.0, y: 2.0, z: 3.0 }).unwrap();
        assert_eq!(s.get::<Position>(p), Some(Position { x: 1.0, y: 2.0, z: 3.0 }));
    }
}
