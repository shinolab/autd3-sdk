use pyo3::prelude::*;

mod client;
mod commands;
mod config;
mod datagram;
mod ops;
mod runtime;
mod stm;

#[pymodule]
fn _autd3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    runtime::init();
    m.add("MAX_IN_FLIGHT", autd3_rs::MAX_IN_FLIGHT)?;
    m.add_class::<client::Client>()?;
    m.add_class::<client::Checker>()?;
    m.add_class::<client::ResponseFuture>()?;
    m.add_class::<client::LinkStatus>()?;
    m.add_class::<client::FpgaState>()?;
    m.add_class::<config::ClientConfig>()?;
    m.add_class::<datagram::DatagramBuilder>()?;
    m.add_class::<datagram::Frames>()?;
    m.add_class::<datagram::Frame>()?;
    m.add_class::<datagram::Pattern>()?;
    m.add_class::<datagram::Modulation>()?;
    m.add_class::<ops::PatternBank>()?;
    m.add_class::<ops::ModulationBank>()?;
    m.add_class::<ops::GpioIn>()?;
    m.add_class::<ops::TransitionMode>()?;
    m.add_class::<ops::LoopBehavior>()?;
    m.add_class::<ops::WritePatternBuffer>()?;
    m.add_class::<ops::PatternCompression>()?;
    m.add_class::<ops::WritePatternCompressed>()?;
    m.add_class::<ops::ConfigPattern>()?;
    m.add_class::<ops::ConfigFociStm>()?;
    m.add_class::<ops::ChangePatternBank>()?;
    m.add_class::<ops::WriteModulationBuffer>()?;
    m.add_class::<ops::ConfigModulation>()?;
    m.add_class::<ops::ChangeModulationBank>()?;
    stm::register(m)?;
    commands::register(m)?;
    Ok(())
}
