use std::future::Future;

pub use tokio::runtime::{Handle, Runtime};

#[must_use]
pub fn new_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .global_queue_interval(1)
        .thread_name("autd3-runtime")
        .enable_all()
        .build()
        .expect("failed to build the tokio runtime")
}

pub fn block_on<F: Future>(handle: &Handle, future: F) -> F::Output {
    tokio::task::block_in_place(|| handle.block_on(future))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn tasks_spawned_from_outside_start_in_the_order_they_were_spawned() {
        let runtime = new_runtime();
        let started = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = channel();

        for task in 0..10_000 {
            let started = Arc::clone(&started);
            let tx = tx.clone();
            runtime.spawn(async move {
                started.lock().unwrap().push(task);
                let _ = tx.send(());
            });
        }
        for _ in 0..10_000 {
            rx.recv().unwrap();
        }

        assert!(started.lock().unwrap().is_sorted());
    }

    #[test]
    fn blocking_works_outside_the_runtime() {
        let runtime = new_runtime();

        assert_eq!(block_on(runtime.handle(), async { 7 }), 7);
    }

    #[test]
    fn blocking_works_on_a_runtime_worker() {
        let runtime = new_runtime();
        let handle = runtime.handle().clone();
        let (tx, rx) = channel();

        runtime.spawn(async move {
            let spawned = block_on(&handle, async { tokio::spawn(async { 7 }).await.unwrap() });
            let _ = tx.send(spawned);
        });

        assert_eq!(rx.recv(), Ok(7));
    }
}
