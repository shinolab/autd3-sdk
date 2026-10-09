use pyo3::prelude::*;

mod client;
mod commands;
mod config;
mod datagram;
mod future;
mod logging;
mod ops;
mod runtime;
mod stm;
mod udp;

#[pymodule]
fn _autd3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(runtime::_shutdown_runtime, m)?)?;
    m.py()
        .import("atexit")?
        .call_method1("register", (m.getattr("_shutdown_runtime")?,))?;
    m.add("MAX_INFLIGHT", autd3_rs::MAX_INFLIGHT)?;
    m.add("MAX_DEVICES", autd3_rs::MAX_DEVICES)?;
    m.add(
        "ULTRASOUND_PERIOD",
        autd3_python_capsule::extract::duration_to_py(m.py(), autd3_rs::common::ULTRASOUND_PERIOD)?,
    )?;
    m.add("MOD_BUFFER_SAMPLES", autd3_rs::params::MOD_BUFFER_SAMPLES)?;
    m.add("BUFFER_SIZE_MIN", autd3_rs::params::BUFFER_SIZE_MIN)?;
    m.add(
        "EMISSION_MAX_INDICES",
        autd3_rs::params::EMISSION_MAX_INDICES,
    )?;
    m.add("NUM_FOCI_MAX", autd3_rs::params::NUM_FOCI_MAX)?;
    m.add("PWE_TABLE_SIZE", autd3_rs::commands::PWE_TABLE_SIZE)?;
    m.add("PULSE_WIDTH_PERIOD", autd3_rs::value::PULSE_WIDTH_PERIOD)?;
    m.add("PITCH_MM", autd3_rs::Autd3::PITCH_MM)?;
    m.add("NUM_TRANSDUCERS", autd3_rs::Autd3::NUM_TRANSDUCERS)?;
    m.add("GRID_X", autd3_rs::Autd3::GRID_X)?;
    m.add("GRID_Y", autd3_rs::Autd3::GRID_Y)?;
    m.add_class::<client::Client>()?;
    m.add_class::<client::StateChecker>()?;
    m.add_class::<client::DeviceState>()?;
    m.add_class::<client::BusStats>()?;
    m.add_class::<client::Version>()?;
    m.add_class::<client::FirmwareVersion>()?;
    m.add_class::<client::Response>()?;
    m.add_class::<client::ResponseFuture>()?;
    m.add_class::<client::StreamFuture>()?;
    m.add_class::<client::DeviceStatus>()?;
    m.add_class::<udp::Interface>()?;
    m.add_class::<udp::TransportOption>()?;
    m.add_class::<udp::UdpEmulator>()?;
    m.add_class::<client::FpgaState>()?;
    m.add_class::<client::TelemetryCounters>()?;
    m.add_class::<config::ClientConfig>()?;
    m.add_class::<datagram::Each>()?;
    m.add_function(wrap_pyfunction!(datagram::each, m)?)?;
    m.add_class::<datagram::Frames>()?;
    m.add_class::<datagram::Frame>()?;
    m.add_class::<datagram::Pattern>()?;
    m.add_class::<datagram::Modulation>()?;
    m.add_class::<ops::PatternBank>()?;
    m.add_class::<ops::ModulationBank>()?;
    m.add_class::<ops::GpioIn>()?;
    m.add_class::<ops::SysTime>()?;
    m.add_class::<ops::TransitionMode>()?;
    m.add_class::<ops::Telemetry>()?;
    m.add_class::<ops::LoopBehavior>()?;
    m.add_class::<ops::WritePatternBuffer>()?;
    m.add_class::<ops::PhaseDepth>()?;
    m.add_class::<ops::WritePatternPhase>()?;
    m.add_class::<ops::ConfigPattern>()?;
    m.add_class::<ops::ConfigFociStm>()?;
    m.add_class::<ops::ActivatePatternBank>()?;
    m.add_class::<ops::WriteModulationBuffer>()?;
    m.add_class::<ops::ConfigModulation>()?;
    m.add_class::<ops::ActivateModulationBank>()?;
    stm::register(m)?;
    logging::register(m)?;
    commands::register(m)?;
    Ok(())
}
