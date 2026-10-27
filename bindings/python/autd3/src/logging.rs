use std::sync::{Mutex, PoisonError};

use autd3_binding_rt::{
    LogWriter as CoreLogWriter, TracingGuard as CoreTracingGuard, TracingOption, try_init_tracing,
};
use pyo3::prelude::*;
use pyo3::sync::MutexExt;

#[pyclass(name = "LogWriter", module = "autd3", eq, hash, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LogWriter(CoreLogWriter);

impl core::hash::Hash for LogWriter {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
    }
}

#[pymethods]
impl LogWriter {
    #[classattr]
    #[pyo3(name = "Stdout")]
    fn stdout() -> Self {
        Self(CoreLogWriter::Stdout)
    }

    #[classattr]
    #[pyo3(name = "Stderr")]
    fn stderr() -> Self {
        Self(CoreLogWriter::Stderr)
    }

    fn __repr__(&self) -> String {
        format!("LogWriter.{:?}", self.0)
    }
}

struct State {
    initialized: bool,
    guard: Option<CoreTracingGuard>,
}

static STATE: Mutex<State> = Mutex::new(State {
    initialized: false,
    guard: None,
});

pub(crate) fn flush() {
    let guard = STATE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .guard
        .take();
    drop(guard);
}

#[pyclass(name = "TracingGuard", module = "autd3", frozen)]
pub struct TracingGuard;

#[pymethods]
impl TracingGuard {
    #[allow(clippy::unused_self)]
    fn close(&self, py: Python<'_>) {
        py.detach(flush);
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (*_args))]
    fn __exit__(&self, py: Python<'_>, _args: &Bound<'_, PyAny>) {
        self.close(py);
    }
}

#[pyfunction]
#[pyo3(signature = (
    default_filter = TracingOption::default().default_filter.to_owned(),
    writer = LogWriter(TracingOption::default().writer),
))]
pub(crate) fn init_tracing(
    py: Python<'_>,
    default_filter: String,
    writer: LogWriter,
) -> PyResult<TracingGuard> {
    let mut state = STATE
        .lock_py_attached(py)
        .unwrap_or_else(PoisonError::into_inner);
    if state.initialized {
        drop(state);
        return Err(autd3_python_capsule::error::generic(
            "tracing is already initialized; init_tracing can be called once per process",
        ));
    }
    let option = TracingOption {
        default_filter: default_filter.leak(),
        writer: writer.0,
    };
    match try_init_tracing(option) {
        Ok(guard) => {
            state.initialized = true;
            state.guard = Some(guard);
            Ok(TracingGuard)
        }
        Err(e) => {
            drop(state);
            Err(autd3_python_capsule::error::generic(e))
        }
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<LogWriter>()?;
    m.add_class::<TracingGuard>()?;
    m.add_function(wrap_pyfunction!(init_tracing, m)?)?;
    Ok(())
}
