use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Wake, Waker};
use std::thread::{JoinHandle, Thread};
use std::time::Duration;

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

#[must_use]
pub fn thread_waker() -> Waker {
    Waker::from(Arc::new(ThreadWaker(std::thread::current())))
}

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

enum Message {
    Poll(Arc<Task>),
    Stop,
}

struct Task {
    future: Mutex<Option<BoxFuture>>,
    queue: Sender<Message>,
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        let queue = self.queue.clone();
        let _ = queue.send(Message::Poll(self));
    }
}

impl Task {
    fn take_future(&self) -> Option<BoxFuture> {
        self.future
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

#[derive(Default)]
struct Registry(Mutex<HashMap<usize, Arc<Task>>>);

impl Registry {
    fn lock(&self) -> MutexGuard<'_, HashMap<usize, Arc<Task>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn key(task: &Arc<Task>) -> usize {
        Arc::as_ptr(task).addr()
    }

    fn insert(&self, task: &Arc<Task>) {
        self.lock().insert(Self::key(task), Arc::clone(task));
    }

    fn remove(&self, task: &Arc<Task>) {
        let removed = self.lock().remove(&Self::key(task));
        drop(removed);
    }

    fn cancel_all(&self) {
        let tasks = std::mem::take(&mut *self.lock());
        for task in tasks.into_values() {
            drop(task.take_future());
        }
    }
}

fn poll_task(task: &Arc<Task>, registry: &Registry) {
    let mut slot = task.future.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(mut future) = slot.take() else {
        return;
    };
    let waker = Waker::from(Arc::clone(task));
    let mut cx = Context::from_waker(&waker);
    if future.as_mut().poll(&mut cx).is_pending() {
        *slot = Some(future);
    } else {
        drop(slot);
        registry.remove(task);
    }
}

struct Worker {
    handle: JoinHandle<()>,
    exited: Receiver<()>,
}

pub struct Executor {
    queue: Mutex<Option<Sender<Message>>>,
    worker: Mutex<Option<Worker>>,
    registry: Arc<Registry>,
}

impl Executor {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = channel::<Message>();
        let (exited_tx, exited_rx) = channel::<()>();
        let registry = Arc::new(Registry::default());
        let tasks = Arc::clone(&registry);
        let handle = std::thread::Builder::new()
            .name("autd3-executor".to_owned())
            .spawn(move || {
                let _exited = exited_tx;
                while let Ok(Message::Poll(task)) = rx.recv() {
                    poll_task(&task, &tasks);
                }
                tasks.cancel_all();
            })
            .expect("failed to spawn the executor thread");
        Self {
            queue: Mutex::new(Some(tx)),
            worker: Mutex::new(Some(Worker {
                handle,
                exited: exited_rx,
            })),
            registry,
        }
    }

    fn lock_queue(&self) -> MutexGuard<'_, Option<Sender<Message>>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn spawn<F: Future<Output = ()> + Send + 'static>(&self, future: F) -> bool {
        let queue = self.lock_queue();
        let Some(sender) = queue.as_ref() else {
            return false;
        };
        let task = Arc::new(Task {
            future: Mutex::new(Some(Box::pin(future))),
            queue: sender.clone(),
        });
        self.registry.insert(&task);
        let sent = sender.send(Message::Poll(Arc::clone(&task))).is_ok();
        if !sent {
            self.registry.remove(&task);
        }
        sent
    }

    fn stop(&self) -> Option<Worker> {
        if let Some(sender) = self.lock_queue().take() {
            let _ = sender.send(Message::Stop);
        }
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        if worker.handle.thread().id() == std::thread::current().id() {
            return None;
        }
        Some(worker)
    }

    pub fn shutdown(&self) {
        if let Some(worker) = self.stop() {
            let _ = worker.handle.join();
        }
    }

    pub fn shutdown_timeout(&self, timeout: Duration) -> bool {
        let Some(worker) = self.stop() else {
            return true;
        };
        if worker.exited.recv_timeout(timeout) == Err(RecvTimeoutError::Timeout) {
            return false;
        }
        let _ = worker.handle.join();
        true
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl core::fmt::Debug for Executor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Executor").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_lock::Semaphore;
    use futures_channel::oneshot;
    use pollster::block_on;

    use super::Executor;

    #[test]
    fn spawned_tasks_run_to_completion() {
        let executor = Executor::new();
        let (tx, rx) = oneshot::channel::<u32>();
        assert!(executor.spawn(async move {
            let _ = tx.send(7);
        }));
        assert_eq!(block_on(rx), Ok(7));
    }

    #[test]
    fn many_tasks_interleave_on_one_thread() {
        let executor = Executor::new();
        let semaphore = Arc::new(Semaphore::new(0));
        let done = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = oneshot::channel::<()>();
        let last = Arc::new(Mutex::new(Some(tx)));
        for _ in 0..16 {
            let semaphore = Arc::clone(&semaphore);
            let done = Arc::clone(&done);
            let last = Arc::clone(&last);
            assert!(executor.spawn(async move {
                semaphore.acquire().await.forget();
                if done.fetch_add(1, Ordering::SeqCst) == 15
                    && let Some(tx) = last.lock().unwrap().take()
                {
                    let _ = tx.send(());
                }
            }));
        }
        for _ in 0..16 {
            semaphore.add_permits(1);
        }
        assert_eq!(block_on(rx), Ok(()));
        assert_eq!(done.load(Ordering::SeqCst), 16);
    }

    #[test]
    fn concurrent_spawns_all_run() {
        const THREADS: usize = 8;
        const TASKS: usize = 64;

        let executor = Arc::new(Executor::new());
        let done = Arc::new(AtomicUsize::new(0));
        let semaphore = Arc::new(Semaphore::new(0));
        let threads = (0..THREADS)
            .map(|_| {
                let executor = Arc::clone(&executor);
                let done = Arc::clone(&done);
                let semaphore = Arc::clone(&semaphore);
                std::thread::spawn(move || {
                    for _ in 0..TASKS {
                        let done = Arc::clone(&done);
                        let semaphore = Arc::clone(&semaphore);
                        assert!(executor.spawn(async move {
                            done.fetch_add(1, Ordering::SeqCst);
                            semaphore.add_permits(1);
                        }));
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }
        for _ in 0..THREADS * TASKS {
            block_on(semaphore.acquire()).forget();
        }
        assert_eq!(done.load(Ordering::SeqCst), THREADS * TASKS);
    }

    #[test]
    fn shutdown_does_not_wait_for_a_pending_task() {
        let executor = Executor::new();
        let semaphore = Arc::new(Semaphore::new(0));
        let started = Arc::new(Semaphore::new(0));
        let blocked = Arc::clone(&semaphore);
        let entered = Arc::clone(&started);
        assert!(executor.spawn(async move {
            entered.add_permits(1);
            blocked.acquire().await.forget();
        }));
        block_on(started.acquire()).forget();
        executor.shutdown();
        assert!(!executor.spawn(async {}));
        semaphore.add_permits(1);
    }

    #[test]
    fn shutdown_drops_pending_tasks() {
        let executor = Executor::new();
        let semaphore = Arc::new(Semaphore::new(0));
        let started = Arc::new(Semaphore::new(0));
        let (tx, rx) = oneshot::channel::<()>();
        let blocked = Arc::clone(&semaphore);
        let entered = Arc::clone(&started);
        assert!(executor.spawn(async move {
            let _tx = tx;
            entered.add_permits(1);
            blocked.acquire().await.forget();
        }));
        block_on(started.acquire()).forget();
        executor.shutdown();
        assert_eq!(block_on(rx), Err(oneshot::Canceled));
        semaphore.add_permits(1);
    }

    #[test]
    fn shutdown_timeout_gives_up_on_a_blocking_poll() {
        let executor = Executor::new();
        let started = Arc::new(Semaphore::new(0));
        let entered = Arc::clone(&started);
        assert!(executor.spawn(async move {
            entered.add_permits(1);
            std::thread::sleep(Duration::from_millis(300));
        }));
        block_on(started.acquire()).forget();
        assert!(!executor.shutdown_timeout(Duration::from_millis(20)));
        assert!(!executor.spawn(async {}));
    }

    #[test]
    fn shutdown_timeout_returns_once_the_worker_exits() {
        let executor = Executor::new();
        assert!(executor.spawn(async {}));
        assert!(executor.shutdown_timeout(Duration::from_secs(5)));
    }

    #[test]
    fn dropping_the_last_handle_inside_a_task_does_not_panic() {
        let executor = Arc::new(Executor::new());
        let release = Arc::new(Semaphore::new(0));
        let (tx, rx) = oneshot::channel::<()>();
        let inner = Arc::clone(&executor);
        let gate = Arc::clone(&release);
        assert!(executor.spawn(async move {
            gate.acquire().await.forget();
            drop(inner);
            let _ = tx.send(());
        }));
        drop(executor);
        release.add_permits(1);
        assert_eq!(block_on(rx), Ok(()));
    }

    #[test]
    fn spawning_after_shutdown_is_rejected() {
        let executor = Executor::new();
        executor.shutdown();
        assert!(!executor.spawn(async {}));
    }

    #[test]
    fn shutdown_is_idempotent() {
        let executor = Executor::new();
        executor.shutdown();
        executor.shutdown();
    }
}
