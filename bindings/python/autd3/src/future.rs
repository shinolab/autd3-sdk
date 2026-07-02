use std::future::Future;

use pyo3::IntoPyObjectExt;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::runtime;

#[pyclass]
struct CheckedCompletor;

#[pymethods]
impl CheckedCompletor {
    #[allow(clippy::unused_self)]
    fn __call__(
        &self,
        future: &Bound<'_, PyAny>,
        complete: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        if future.call_method0("cancelled")?.is_truthy()? {
            return Ok(());
        }
        complete.call1((value,))?;
        Ok(())
    }
}

fn set_result(
    event_loop: &Bound<'_, PyAny>,
    future: &Bound<'_, PyAny>,
    result: PyResult<Py<PyAny>>,
) -> PyResult<()> {
    let py = event_loop.py();
    let (complete, val) = match result {
        Ok(val) => (future.getattr("set_result")?, val),
        Err(err) => (future.getattr("set_exception")?, err.into_py_any(py)?),
    };
    event_loop.call_method1(
        "call_soon_threadsafe",
        (CheckedCompletor, future, complete, val),
    )?;
    Ok(())
}

pub(crate) fn future_into_py<'py, F, T>(py: Python<'py>, fut: F) -> PyResult<Bound<'py, PyAny>>
where
    F: Future<Output = PyResult<T>> + Send + 'static,
    T: for<'p> IntoPyObject<'p> + Send + 'static,
{
    let event_loop = py.import("asyncio")?.call_method0("get_running_loop")?;
    let py_fut = event_loop.call_method0("create_future")?;
    let event_loop_tx = event_loop.unbind();
    let future_tx = py_fut.clone().unbind();
    let handle = runtime::handle()?;
    let completion_handle = handle.clone();
    let inner = handle.spawn(fut);
    drop(handle.spawn(async move {
        let result = match inner.await {
            Ok(result) => result,
            Err(e) => Err(PyRuntimeError::new_err(format!("rust future failed: {e}"))),
        };
        drop(completion_handle.spawn_blocking(move || {
            Python::attach(|py| {
                let result = result.and_then(|val| val.into_py_any(py));
                let _ = set_result(event_loop_tx.bind(py), future_tx.bind(py), result);
            });
        }));
    }));
    Ok(py_fut)
}
