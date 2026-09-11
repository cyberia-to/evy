use super::*;
use bbg::storage::{MemStore, ShardStore};
use evy_ecs_storage::{Goldilocks, StorageError, StorageResult};
use evy_engine_dispatch::{CommitPolicy, Engine as EngineKind, ShardRef};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct UnknownCommit(MemStore);
impl ShardStore for UnknownCommit {
    fn get(&self, d: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        self.0.get(d, key)
    }
    fn put(&mut self, d: u8, key: [u8; 32], value: Vec<Goldilocks>) -> StorageResult<()> {
        self.0.put(d, key, value)
    }
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        self.0.dirty_entries()
    }
    fn commit(&mut self) -> StorageResult<[u8; 32]> {
        Err(StorageError::CommitUnknown {
            change_id: [9; 32],
            message: "injected response loss".into(),
        })
    }
    fn get_mut(&mut self, d: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        self.0.get_mut(d, key)
    }
    fn mark_dirty(&mut self, d: u8, key: [u8; 32]) -> StorageResult<()> {
        self.0.mark_dirty(d, key)
    }
    fn remove(&mut self, d: u8, key: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.0.remove(d, key)
    }
    fn iter(&self, d: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        self.0.iter(d)
    }
}

#[test]
fn unknown_commit_reaches_caller_without_advancing_tick() {
    let mut engine = Engine::builder().without_radio().build().unwrap();
    engine.storage = ShardStorage::with_backend(Box::new(UnknownCommit(MemStore::new())));
    assert_eq!(
        engine.commit_tick(),
        Err(StorageError::CommitUnknown {
            change_id: [9; 32],
            message: "injected response loss".into(),
        })
    );
    assert_eq!(engine.tick(), 0);
}

struct CheckedNode {
    ran: Arc<AtomicBool>,
    fail: bool,
}
impl DispatchNode for CheckedNode {
    fn engine(&self) -> EngineKind {
        EngineKind::Cpu
    }
    fn reads(&self) -> &[ShardRef] {
        &[]
    }
    fn writes(&self) -> &[ShardRef] {
        &[]
    }
    fn commit_policy(&self) -> CommitPolicy {
        CommitPolicy::None
    }
    fn dispatch(&mut self, _: &mut DispatchCtx<'_>) -> Result<(), DispatchError> {
        self.ran.store(true, Ordering::Relaxed);
        if self.fail {
            Err(StorageError::Io("injected dispatch read failure".into()).into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn dispatch_storage_failure_stops_successors_without_advancing_frame() {
    let mut engine = Engine::builder().without_radio().build().unwrap();
    let first = Arc::new(AtomicBool::new(false));
    let successor = Arc::new(AtomicBool::new(false));
    engine.add_node(Box::new(CheckedNode {
        ran: first.clone(),
        fail: true,
    }));
    engine.add_node(Box::new(CheckedNode {
        ran: successor.clone(),
        fail: false,
    }));
    assert!(matches!(
        engine.run_frame(),
        Err(DispatchError::Storage(StorageError::Io(_)))
    ));
    assert!(first.load(Ordering::Relaxed));
    assert!(!successor.load(Ordering::Relaxed));
    assert_eq!(engine.frame(), 0);
}
