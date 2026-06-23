use pyo3::prelude::*;

mod client;
mod config;
mod datagram;
mod ops;
mod runtime;

#[pymodule]
fn _autd3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    runtime::init();
    m.add_class::<client::Client>()?;
    m.add_class::<client::LinkStatus>()?;
    m.add_class::<config::ClientConfig>()?;
    m.add_class::<datagram::DatagramBuilder>()?;
    m.add_class::<datagram::Datagrams>()?;
    m.add_class::<datagram::Frame>()?;
    m.add_class::<datagram::Pattern>()?;
    m.add_class::<datagram::Modulation>()?;
    m.add_class::<ops::PatternBank>()?;
    m.add_class::<ops::ModulationBank>()?;
    m.add_class::<ops::PatternDataType>()?;
    m.add_class::<ops::TransitionMode>()?;
    m.add_class::<ops::WritePatternBuffer>()?;
    m.add_class::<ops::ConfigPattern>()?;
    m.add_class::<ops::ChangePatternBank>()?;
    m.add_class::<ops::WriteModulationBuffer>()?;
    m.add_class::<ops::ConfigModulation>()?;
    m.add_class::<ops::ChangeModulationBank>()?;
    Ok(())
}
