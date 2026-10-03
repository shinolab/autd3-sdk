use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyByteArray, PyBytes};

static NDARRAY: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static MASKED_ARRAY: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static UINT8: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static FLOAT32: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static FROMBUFFER: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

fn ndarray_type(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    NDARRAY.import(py, "numpy", "ndarray")
}

fn masked_array_type(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    MASKED_ARRAY.import(py, "numpy.ma", "MaskedArray")
}

fn cached_dtype<'py>(
    py: Python<'py>,
    cell: &'static PyOnceLock<Py<PyAny>>,
    name: &str,
) -> PyResult<&'py Bound<'py, PyAny>> {
    cell.get_or_try_init(py, || {
        Ok::<_, PyErr>(
            py.import("numpy")?
                .getattr("dtype")?
                .call1((name,))?
                .unbind(),
        )
    })
    .map(|d| d.bind(py))
}

fn uint8_dtype(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    cached_dtype(py, &UINT8, "uint8")
}

fn float32_dtype(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    cached_dtype(py, &FLOAT32, "float32")
}

fn frombuffer(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    FROMBUFFER.import(py, "numpy", "frombuffer")
}

pub fn is_ndarray(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(ndarray_type(obj.py())?)
}

pub fn u8_matrix_bytes<'py>(
    obj: &Bound<'py, PyAny>,
    cols: usize,
    rows: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let py = obj.py();
    if !is_ndarray(obj)? {
        return Err(PyTypeError::new_err(format!(
            "expected a numpy.ndarray, got {}",
            obj.get_type().name()?
        )));
    }
    if obj.is_instance(masked_array_type(py)?)? {
        return Err(PyTypeError::new_err(
            "masked arrays are not supported; fill them explicitly (e.g. arr.filled(0))",
        ));
    }
    let dtype = obj.getattr("dtype")?;
    if !dtype.eq(uint8_dtype(py)?)? {
        return Err(PyTypeError::new_err(format!(
            "expected dtype uint8, got {dtype}; convert explicitly (e.g. arr.astype(numpy.uint8))"
        )));
    }
    let shape_obj = obj.getattr("shape")?;
    let shape: Vec<usize> = shape_obj.extract()?;
    let matches = match (rows, shape.as_slice()) {
        (Some(n), &[r, c]) => r == n && c == cols,
        (None, &[_, c]) => c == cols,
        _ => false,
    };
    if !matches {
        let expected_rows = rows.map_or_else(|| "n".to_owned(), |n| n.to_string());
        return Err(PyValueError::new_err(format!(
            "expected shape ({expected_rows}, {cols}), got {shape_obj}"
        )));
    }
    obj.call_method0("tobytes")?
        .cast_into::<PyBytes>()
        .map_err(Into::into)
}

pub fn u8_matrix(
    py: Python<'_>,
    rows: usize,
    cols: usize,
    fill: impl FnOnce(&mut [u8]),
) -> PyResult<Bound<'_, PyAny>> {
    let buf = PyByteArray::new_with(py, rows * cols, |b| {
        fill(b);
        Ok(())
    })?;
    frombuffer(py)?
        .call1((buf, uint8_dtype(py)?))?
        .call_method1("reshape", (rows, cols))
}

pub fn f32_vec3_rows(
    py: Python<'_>,
    rows: impl ExactSizeIterator<Item = [f32; 3]>,
) -> PyResult<Bound<'_, PyAny>> {
    let n = rows.len();
    let buf = PyByteArray::new_with(py, n * 3 * size_of::<f32>(), |b| {
        for (dst, v) in b
            .as_chunks_mut::<{ size_of::<f32>() }>()
            .0
            .iter_mut()
            .zip(rows.flatten())
        {
            *dst = v.to_ne_bytes();
        }
        Ok(())
    })?;
    frombuffer(py)?
        .call1((buf, float32_dtype(py)?))?
        .call_method1("reshape", (n, 3))
}
