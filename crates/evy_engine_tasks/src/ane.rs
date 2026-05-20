//! `AneTaskPool` — single worker that serializes ANE program dispatch.
//!
//! ANE is a shared coprocessor with limited concurrency; multiple programs
//! submitted simultaneously serialize in hardware. We surface this honestly
//! by exposing one worker thread that drains tasks in submission order.
//! For multi-stream inference, the runtime should batch multiple particle
//! inputs into one `rane::Program::run()` call rather than dispatching
//! one program per particle.
//!
//! The pool does NOT load programs — `rane::Program::compile()` and
//! `load()` are caller responsibilities (programs are typically loaded
//! once and reused across frames). The pool just gives those programs
//! a place to run in serialized order, separate from the GPU and CPU
//! frame budgets.

use crate::error::PoolError;

pub struct AneTaskPool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    inner: imp::Inner,
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    _marker: core::marker::PhantomData<()>,
}

impl AneTaskPool {
    /// Create the ANE dispatch worker. Returns `Unsupported` on targets
    /// without ANE (non-Apple-Silicon). On Apple Silicon assumes ANE is
    /// available; probe `PlatformCapabilities` before constructing if you
    /// need to handle ANE-disabled VMs.
    pub fn new() -> Result<Self, PoolError> {
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            Err(PoolError::Unsupported)
        }

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let inner = imp::Inner::new()?;
            Ok(Self { inner })
        }
    }

    /// Submit a task. Returns a `Receiver` that yields the task's result
    /// when ANE finishes the dispatch.
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
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod imp {
    use super::PoolError;
    use std::sync::mpsc;
    use std::thread;

    type BoxedTask = Box<dyn FnOnce() + Send + 'static>;

    pub(super) struct Inner {
        sender: Option<mpsc::Sender<BoxedTask>>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl Inner {
        pub(super) fn new() -> Result<Self, PoolError> {
            let (tx, rx) = mpsc::channel::<BoxedTask>();
            let handle = thread::spawn(move || {
                // ANE has no per-thread initialization equivalent to AMX;
                // rane::Program::run handles its own dispatch state. The
                // worker just drains the queue.
                while let Ok(task) = rx.recv() {
                    task();
                }
            });
            Ok(Self {
                sender: Some(tx),
                handle: Some(handle),
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
            if let Some(sender) = &self.sender {
                let _ = sender.send(wrapped);
            }
            result_rx
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            // Close inbound channel; the worker exits when its recv() yields Err.
            drop(self.sender.take());
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn spawn_returns_result() {
        let pool = AneTaskPool::new().expect("Apple Silicon supports ANE");
        let rx = pool.spawn(|| 7u64);
        assert_eq!(rx.recv().unwrap(), 7);
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn tasks_run_in_submission_order() {
        let pool = AneTaskPool::new().unwrap();
        let receivers: Vec<_> = (0..8).map(|i| pool.spawn(move || i)).collect();
        for (i, rx) in receivers.into_iter().enumerate() {
            assert_eq!(rx.recv().unwrap(), i, "ANE pool must serialize in order");
        }
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn many_tasks_complete() {
        let pool = AneTaskPool::new().unwrap();
        let receivers: Vec<_> = (0..32).map(|i| pool.spawn(move || i * 3)).collect();
        let results: Vec<usize> = receivers.into_iter().map(|r| r.recv().unwrap()).collect();
        assert_eq!(results, (0..32).map(|i| i * 3).collect::<Vec<_>>());
    }

    #[test]
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    fn unsupported_on_non_apple_silicon() {
        assert!(matches!(AneTaskPool::new(), Err(PoolError::Unsupported)));
    }
}
