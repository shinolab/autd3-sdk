use pyo3::prelude::*;

mod error;
mod geometry;
mod value;

#[pymodule]
fn autd3_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("Autd3Error", m.py().get_type::<error::Autd3Error>())?;
    m.add_class::<value::Intensity>()?;
    m.add_class::<value::Phase>()?;
    m.add_class::<value::Emission>()?;
    m.add_class::<value::SamplingConfig>()?;
    m.add_class::<geometry::Autd3>()?;
    m.add_class::<geometry::Geometry>()?;
    m.add_function(wrap_pyfunction!(geometry::_read_geometry_capsule, m)?)?;
    Ok(())
}
