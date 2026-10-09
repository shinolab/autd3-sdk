use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyByteArray, PyBytes};

static NDARRAY: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static MASKED_ARRAY: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static UINT8: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static FLOAT32: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static FROMBUFFER: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

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

fn frombuffer(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    FROMBUFFER.import(py, "numpy", "frombuffer")
}

pub fn is_ndarray(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(NDARRAY.import(obj.py(), "numpy", "ndarray")?)
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
    if obj.is_instance(MASKED_ARRAY.import(py, "numpy.ma", "MaskedArray")?)? {
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
        .call1((buf, cached_dtype(py, &FLOAT32, "float32")?))?
        .call_method1("reshape", (n, 3))
}

pub fn u8_vector_bytes<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let py = obj.py();
    if !is_ndarray(obj)? {
        return Err(PyTypeError::new_err(format!(
            "expected a numpy.ndarray, got {}",
            obj.get_type().name()?
        )));
    }
    if obj.is_instance(MASKED_ARRAY.import(py, "numpy.ma", "MaskedArray")?)? {
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
    if shape.len() != 1 {
        return Err(PyValueError::new_err(format!(
            "expected shape (n,), got {shape_obj}"
        )));
    }
    obj.call_method0("tobytes")?
        .cast_into::<PyBytes>()
        .map_err(Into::into)
}

pub fn u8_vector<'py>(py: Python<'py>, values: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let buf = PyByteArray::new(py, values);
    frombuffer(py)?.call1((buf, uint8_dtype(py)?))
}

pub fn f32_vector<const N: usize>(py: Python<'_>, v: [f32; N]) -> PyResult<Bound<'_, PyAny>> {
    let buf = PyByteArray::new_with(py, N * size_of::<f32>(), |b| {
        for (dst, v) in b
            .as_chunks_mut::<{ size_of::<f32>() }>()
            .0
            .iter_mut()
            .zip(v)
        {
            *dst = v.to_ne_bytes();
        }
        Ok(())
    })?;
    frombuffer(py)?.call1((buf, cached_dtype(py, &FLOAT32, "float32")?))
}

pub fn f32_vec3(py: Python<'_>, v: [f32; 3]) -> PyResult<Bound<'_, PyAny>> {
    f32_vector(py, v)
}

pub fn u8_vector_dst<'a, 'py>(
    obj: &'a Bound<'py, PyAny>,
    len: usize,
) -> PyResult<&'a Bound<'py, PyAny>> {
    let py = obj.py();
    if !is_ndarray(obj)? {
        return Err(PyTypeError::new_err(format!(
            "expected a numpy.ndarray, got {}",
            obj.get_type().name()?
        )));
    }
    if obj.is_instance(MASKED_ARRAY.import(py, "numpy.ma", "MaskedArray")?)? {
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
    if shape != [len] {
        return Err(PyValueError::new_err(format!(
            "expected shape ({len},), got {shape_obj}"
        )));
    }
    let flags = obj.getattr("flags")?;
    if !flags.getattr("c_contiguous")?.extract::<bool>()? {
        return Err(PyValueError::new_err(
            "expected a C-contiguous array; copy it first (e.g. numpy.ascontiguousarray(arr))",
        ));
    }
    if !flags.getattr("writeable")?.extract::<bool>()? {
        return Err(PyValueError::new_err("expected a writeable array"));
    }
    Ok(obj)
}

pub fn u8_vector_write(dst: &Bound<'_, PyAny>, values: &[u8]) -> PyResult<()> {
    static COPYTO: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let py = dst.py();
    let src = frombuffer(py)?.call1((PyBytes::new(py, values), uint8_dtype(py)?))?;
    COPYTO.import(py, "numpy", "copyto")?.call1((dst, src))?;
    Ok(())
}
