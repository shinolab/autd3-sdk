pub use tokio::runtime::Runtime;

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
}
