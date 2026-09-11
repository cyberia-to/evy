use super::*;
use crate::{Namespace, StorageError};

#[derive(Debug, PartialEq)]
struct Value(u64);
impl EvyComponent for Value {
    const NAMESPACE: Namespace = Namespace::Coins;
    fn to_goldilocks(&self) -> Vec<Goldilocks> {
        vec![Goldilocks::new(self.0)]
    }
    fn from_goldilocks(value: &[Goldilocks]) -> Self {
        Self(value[0].as_u64())
    }
}

struct FailedBackend;
fn fault<T>() -> StorageResult<T> {
    Err(StorageError::Io("injected backend error".into()))
}
impl ShardStore for FailedBackend {
    fn get(&self, _: u8, _: &[u8; 32]) -> Option<&[Goldilocks]> {
        None
    }
    fn put(&mut self, _: u8, _: [u8; 32], _: Vec<Goldilocks>) -> StorageResult<()> {
        fault()
    }
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &[]
    }
    fn commit(&mut self) -> StorageResult<[u8; 32]> {
        Err(StorageError::CommitUnknown {
            change_id: [7; 32],
            message: "lost flush response".into(),
        })
    }
    fn get_mut(&mut self, _: u8, _: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        None
    }
    fn mark_dirty(&mut self, _: u8, _: [u8; 32]) -> StorageResult<()> {
        fault()
    }
    fn remove(&mut self, _: u8, _: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>> {
        fault()
    }
    fn read(&self, _: u8, _: &[u8; 32], _: usize) -> StorageResult<Option<Vec<Goldilocks>>> {
        fault()
    }
    fn scan(
        &self,
        _: u8,
        _: Option<[u8; 32]>,
        _: ScanLimits,
    ) -> StorageResult<Vec<bbg::storage::ShardEntry>> {
        fault()
    }
    fn iter(&self, _: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn storage_failures_remain_distinct_from_absence_and_success() {
    let mut storage = ShardStorage::with_backend(Box::new(FailedBackend));
    let particle = ParticleId::from_entity(1, 0);
    assert!(matches!(
        storage.put(particle, &Value(10)),
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.mark_dirty::<Value>(particle),
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.remove::<Value>(particle),
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.read::<Value>(particle, 1),
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.scan::<Value>(
            None,
            ScanLimits {
                max_entries: 1,
                max_elements: 1
            }
        ),
        Err(StorageError::Io(_))
    ));
    assert_eq!(
        storage.commit(),
        Err(StorageError::CommitUnknown {
            change_id: [7; 32],
            message: "lost flush response".into(),
        })
    );
}

#[test]
fn typed_owned_reads_and_scans_preserve_budgets_and_cursor() -> StorageResult<()> {
    let mut storage = ShardStorage::with_backend(Box::new(MemStore::new()));
    let first = ParticleId::from_hash([1; 32]);
    let second = ParticleId::from_hash([2; 32]);
    storage.put(first, &Value(10))?;
    storage.put(second, &Value(20))?;
    assert!(matches!(
        storage.read::<Value>(first, 0),
        Err(StorageError::Limit(_))
    ));
    assert_eq!(storage.read::<Value>(first, 1)?, Some(Value(10)));
    assert_eq!(
        storage.scan::<Value>(
            Some(first),
            ScanLimits {
                max_entries: 1,
                max_elements: 1
            }
        )?,
        vec![(second, Value(20))]
    );
    Ok(())
}
