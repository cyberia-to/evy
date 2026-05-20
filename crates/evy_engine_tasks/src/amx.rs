//! `AmxTaskPool` — N pinned threads, each holding an Apple AMX context for
//! its lifetime.
//!
//! AMX requires a per-thread context (`acpu::matrix::Matrix`) that is `!Send`
//! and `!Sync`. Worker threads activate AMX once at start and keep it active
//! until the pool drops. Tasks submitted to the pool run on a worker that
//! already has AMX live — they can call `acpu::matmul_f32` and friends
//! without worrying about activation/deactivation.

use crate::error::PoolError;

/// Apple AMX worker pool. Round-robin task distribution across N workers.
pub struct AmxTaskPool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    inner: imp::Inner,
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    _marker: core::marker::PhantomData<()>,
}

impl AmxTaskPool {
    /// Create a pool with `num_workers` worker threads, each with AMX active.
    ///
    /// On non-Apple-Silicon targets returns `PoolError::Unsupported`. If
    /// `num_workers == 0` returns `PoolError::Unsupported` (no work can run).
    pub fn new(num_workers: usize) -> Result<Self, PoolError> {
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            let _ = num_workers;
            Err(PoolError::Unsupported)
        }

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if num_workers == 0 {
                return Err(PoolError::Unsupported);
            }
            let inner = imp::Inner::new(num_workers)?;
            Ok(Self { inner })
        }
    }

    /// Submit a task. Returns a `Receiver` that yields the task's result
    /// when the worker finishes.
    ///
    /// On unsupported targets the receiver is closed immediately and
    /// `recv()` returns `Err`.
    pub fn spawn<F, T>(&self, task: F) -> std::sync::mpsc::Receiver<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            let _ = task;
            let (_, rx) = std::sync::mpsc::channel();
            rx
        }

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            self.inner.spawn(task)
        }
    }

    /// Number of workers in the pool. 0 on unsupported targets.
    pub fn worker_count(&self) -> usize {
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            0
        }

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            self.inner.workers.len()
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod imp {
    use super::PoolError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::thread;

    type BoxedTask = Box<dyn FnOnce() + Send + 'static>;

    pub(super) struct Inner {
        pub(super) workers: Vec<Worker>,
        next: AtomicUsize,
    }

    pub(super) struct Worker {
        sender: mpsc::Sender<BoxedTask>,
        // Handle kept so the thread joins when the pool drops.
        handle: Option<thread::JoinHandle<()>>,
    }

    impl Inner {
        pub(super) fn new(num_workers: usize) -> Result<Self, PoolError> {
            let mut workers = Vec::with_capacity(num_workers);
            for _ in 0..num_workers {
                let (tx, rx) = mpsc::channel::<BoxedTask>();
                let handle = thread::spawn(move || {
                    // Activate AMX for the lifetime of this worker.
                    // If activation fails the thread exits silently — the
                    // pool drains tasks via the closed sender. Probe
                    // capabilities before constructing the pool to avoid this.
                    let _matrix = match acpu::matrix::Matrix::new() {
                        Ok(m) => m,
                        Err(_) => return,
                    };
                    while let Ok(task) = rx.recv() {
                        task();
                    }
                    // _matrix drops here, calling AMX_CLR on this thread.
                });
                workers.push(Worker {
                    sender: tx,
                    handle: Some(handle),
                });
            }
            Ok(Self {
                workers,
                next: AtomicUsize::new(0),
            })
        }

        pub(super) fn spawn<F, T>(&self, task: F) -> mpsc::Receiver<T>
        where
            F: FnOnce() -> T + Send + 'static,
            T: Send + 'static,
        {
            let (result_tx, result_rx) = mpsc::channel::<T>();
            let wrapped: BoxedTask = Box::new(move || {
                let out = task();
                let _ = result_tx.send(out);
            });
            let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.workers.len();
            // If the worker's receiver dropped (shouldn't happen until pool
            // drop), the send fails silently; result_rx will also yield Err
            // on recv. Caller handles both cases the same way.
            let _ = self.workers[idx].sender.send(wrapped);
            result_rx
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            // Drain workers by value; for each, drop its Sender (closes the
            // worker's inbound channel — recv() returns Err and the worker
            // exits its loop, dropping its AMX Matrix) and join the thread.
            for w in self.workers.drain(..) {
                let Worker { sender, mut handle } = w;
                drop(sender);
                if let Some(h) = handle.take() {
                    let _ = h.join();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn spawn_returns_task_result() {
        let pool = AmxTaskPool::new(2).expect("Apple Silicon supports AMX");
        let rx = pool.spawn(|| 42u32);
        assert_eq!(rx.recv().unwrap(), 42);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn many_tasks_distribute_across_workers() {
        let pool = AmxTaskPool::new(4).unwrap();
        let receivers: Vec<_> = (0..16).map(|i| pool.spawn(move || i * 2)).collect();
        let mut results: Vec<u32> = receivers.into_iter().map(|r| r.recv().unwrap()).collect();
        results.sort();
        assert_eq!(results, (0..16).map(|i| i * 2).collect::<Vec<_>>());
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn worker_count_matches_request() {
        let pool = AmxTaskPool::new(3).unwrap();
        assert_eq!(pool.worker_count(), 3);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn zero_workers_is_unsupported() {
        assert!(matches!(AmxTaskPool::new(0), Err(PoolError::Unsupported)));
    }

    #[test]
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    fn unsupported_on_non_apple_silicon() {
        assert!(matches!(AmxTaskPool::new(4), Err(PoolError::Unsupported)));
    }
}
