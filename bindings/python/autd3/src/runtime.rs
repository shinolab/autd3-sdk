use std::future::Future;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use autd3_rs::rt::Executor;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

type Completion = Box<dyn FnOnce() + Send + 'static>;

#[derive(Clone)]
pub(crate) struct Completions(Sender<Option<Completion>>);

impl Completions {
    pub(crate) fn post(&self, completion: impl FnOnce() + Send + 'static) {
        let _ = self.0.send(Some(Box::new(completion)));
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

fn with_runtime<R>(f: impl FnOnce(&Runtime) -> Option<R>) -> PyResult<R> {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if matches!(*state, State::Uninit) {
        *state = State::Running(Runtime::new());
    }
    match &*state {
        State::Running(runtime) => f(runtime).ok_or_else(shutdown_error),
        State::Shutdown => Err(shutdown_error()),
        State::Uninit => unreachable!(),
    }
}

pub(crate) fn completions() -> PyResult<Completions> {
    with_runtime(|runtime| Some(runtime.completions.clone()))
}

pub(crate) fn spawn<F: Future<Output = ()> + Send + 'static>(future: F) -> PyResult<()> {
    with_runtime(|runtime| runtime.executor.spawn(future).then_some(()))
}

#[pyfunction]
pub(crate) fn _shutdown_runtime(py: Python<'_>) {
    py.detach(crate::udp::shutdown_emulators);
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
    py.detach(crate::logging::flush);
}
