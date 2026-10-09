use pyo3::prelude::*;

mod error;
mod geometry;
mod units;
mod value;

#[pymodule]
fn autd3_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    error::register(m)?;
    m.add_class::<value::Intensity>()?;
    m.add_class::<value::Phase>()?;
    m.add_class::<value::SamplingConfig>()?;
    m.add_class::<value::PyNearest>()?;
    m.add_class::<value::Duration>()?;
    m.add_class::<geometry::Autd3>()?;
    m.add_class::<geometry::EulerAngles>()?;
    m.add_class::<geometry::Geometry>()?;
    m.add_class::<geometry::Device>()?;
    m.add_function(wrap_pyfunction!(geometry::point, m)?)?;
    m.add_function(wrap_pyfunction!(geometry::offset, m)?)?;
    units::register(m)?;
    Ok(())
}
