use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyType};

fn define<'py>(
    m: &Bound<'py, PyModule>,
    name: &str,
    bases: &[Bound<'py, PyAny>],
) -> PyResult<Bound<'py, PyAny>> {
    let py = m.py();
    let namespace = PyDict::new(py);
    namespace.set_item("__module__", "autd3_core")?;
    let class = py
        .get_type::<PyType>()
        .call1((name, PyTuple::new(py, bases)?, namespace))?;
    m.add(name, &class)?;
    Ok(class)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let builtins = py.import("builtins")?;
    let base = define(m, "Autd3Error", &[builtins.getattr("Exception")?])?;
    define(
        m,
        "InvalidArgumentError",
        &[base.clone(), builtins.getattr("ValueError")?],
    )?;
    define(
        m,
        "ReplyTimeoutError",
        &[base.clone(), builtins.getattr("TimeoutError")?],
    )?;
    define(m, "DeviceError", std::slice::from_ref(&base))?;
    define(m, "NetworkError", std::slice::from_ref(&base))?;
    Ok(())
}
