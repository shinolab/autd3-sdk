use std::future::Future;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use autd3_rs::rt::Executor;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

type Completion = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct Completions(Sender<Option<Completion>>);

impl Completions {
    pub(crate) fn post(&self, completion: impl FnOnce() + Send + 'static) {
        let _ = self.0.send(Some(Box::new(completion)));
    }
}

impl Clone for Completions {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

struct Runtime {
    executor: Executor,
    completions: Completions,
    completion_thread: Option<JoinHandle<()>>,
}

impl Runtime {
    fn new() -> Self {
        let (tx, rx) = channel::<Option<Completion>>();
        let completion_thread = std::thread::Builder::new()
            .name("autd3-py-completions".to_owned())
            .spawn(move || {
                while let Ok(Some(completion)) = rx.recv() {
                    completion();
                }
            })
            .expect("failed to spawn the completion thread");
        Self {
            executor: Executor::new(),
            completions: Completions(tx),
            completion_thread: Some(completion_thread),
        }
    }

    fn shutdown(mut self) {
        let _ = self.executor.shutdown_timeout(SHUTDOWN_TIMEOUT);
        let _ = self.completions.0.send(None);
        if let Some(thread) = self.completion_thread.take() {
            let _ = thread.join();
        }
    }
}

enum State {
    Uninit,
    Running(Runtime),
    Shutdown,
}

static STATE: Mutex<State> = Mutex::new(State::Uninit);

fn shutdown_error() -> PyErr {
    PyRuntimeError::new_err("autd3 runtime has been shut down")
}

pub(crate) fn completions() -> PyResult<Completions> {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if matches!(*state, State::Uninit) {
        *state = State::Running(Runtime::new());
    }
    match &*state {
        State::Running(runtime) => Ok(runtime.completions.clone()),
        State::Shutdown => Err(shutdown_error()),
        State::Uninit => unreachable!(),
    }
}

pub(crate) fn spawn<F: Future<Output = ()> + Send + 'static>(future: F) -> PyResult<()> {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if matches!(*state, State::Uninit) {
        *state = State::Running(Runtime::new());
    }
    match &*state {
        State::Running(runtime) if runtime.executor.spawn(future) => Ok(()),
        State::Running(_) | State::Shutdown => Err(shutdown_error()),
        State::Uninit => unreachable!(),
    }
}

#[pyfunction]
pub(crate) fn _shutdown_runtime(py: Python<'_>) {
    let runtime = {
        let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
        match std::mem::replace(&mut *state, State::Shutdown) {
            State::Running(runtime) => Some(runtime),
            State::Uninit | State::Shutdown => None,
        }
    };
    if let Some(runtime) = runtime {
        py.detach(|| runtime.shutdown());
    }
}
