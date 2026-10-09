use core::num::{NonZeroU16, NonZeroU32};
use core::time::Duration;
use std::sync::Arc;

use autd3_python_capsule::extract::{duration_to_py, extract_duration, extract_u8};
use autd3_rs::commands::Expansion;
use autd3_rs::commands::{
    Clear as CoreClear, CpuConfig as CoreCpuConfig, EmulateGpioIn, FixedCompletionTime,
    FixedUpdateRate, ForceFan as CoreForceFan, FpgaBusWait as CoreFpgaBusWait,
    GpioOut as CoreGpioOut, Nop as CoreNop, PWE_TABLE_SIZE, PtpConfig as CorePtpConfig,
    ReleaseFailsafe as CoreReleaseFailsafe, SetCpuConfig as CoreSetCpuConfig, SetGpioOut,
    SetOutputMask, SetPhaseCorrection, SetPulseWidthTable as CoreSetPulseWidthTable, SetSilencer,
    Synchronize as CoreSynchronize,
};
use autd3_rs::geometry::Autd3;
use autd3_rs::value::{Phase, PulseWidth as CorePulseWidth};

use crate::datagram::PushCommand;
use crate::ops::SysTime;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

macro_rules! simple_command {
    ($pyname:literal, $py:ident, $core:ident) => {
        #[pyclass(name = $pyname, module = "autd3.commands", frozen)]
        pub struct $py(pub(crate) Arc<$core>);

        #[pymethods]
        impl $py {
            #[new]
            fn new() -> Self {
                Self(Arc::new($core))
            }
        }
    };
}

simple_command!("Clear", Clear, CoreClear);
simple_command!("Synchronize", Synchronize, CoreSynchronize);
simple_command!("ReleaseFailsafe", ReleaseFailsafe, CoreReleaseFailsafe);
simple_command!("Nop", Nop, CoreNop);

#[pyclass(name = "ForceFan", module = "autd3.commands", frozen)]
pub struct ForceFan(pub(crate) Arc<CoreForceFan>);

#[pymethods]
impl ForceFan {
    #[new]
    fn new(value: bool) -> Self {
        Self(Arc::new(CoreForceFan { value }))
    }
}

#[pyclass(
    name = "FixedCompletionTime",
    module = "autd3.commands",
    skip_from_py_object
)]
pub struct FixedCompletionTimePy(FixedCompletionTime);

#[pymethods]
impl FixedCompletionTimePy {
    #[new]
    #[pyo3(signature = (intensity = None, phase = None, strict_mode = true))]
    fn new(
        intensity: Option<&Bound<'_, PyAny>>,
        phase: Option<&Bound<'_, PyAny>>,
        strict_mode: bool,
    ) -> PyResult<Self> {
        let default = FixedCompletionTime::default();
        Ok(Self(FixedCompletionTime {
            intensity: intensity
                .map(extract_duration)
                .transpose()?
                .unwrap_or(default.intensity),
            phase: phase
                .map(extract_duration)
                .transpose()?
                .unwrap_or(default.phase),
            strict_mode,
        }))
    }
}

#[pyclass(
    name = "FixedUpdateRate",
    module = "autd3.commands",
    skip_from_py_object
)]
pub struct FixedUpdateRatePy(FixedUpdateRate);

#[pymethods]
impl FixedUpdateRatePy {
    #[new]
    #[pyo3(signature = (intensity, phase))]
    fn new(intensity: u16, phase: u16) -> PyResult<Self> {
        Ok(Self(FixedUpdateRate {
            intensity: NonZeroU16::new(intensity)
                .ok_or_else(|| PyValueError::new_err("intensity must be >= 1"))?,
            phase: NonZeroU16::new(phase)
                .ok_or_else(|| PyValueError::new_err("phase must be >= 1"))?,
        }))
    }
}

#[pyclass(name = "SetSilencer", module = "autd3.commands", frozen)]
pub struct SetSilencerPy(pub(crate) Arc<SetSilencer>);

#[pymethods]
impl SetSilencerPy {
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let inner = match config {
            None => SetSilencer::default(),
            Some(config) => {
                if let Ok(c) = config.cast::<FixedCompletionTimePy>() {
                    SetSilencer::new(c.borrow().0)
                } else if let Ok(c) = config.cast::<FixedUpdateRatePy>() {
                    SetSilencer::new(c.borrow().0)
                } else {
                    return Err(PyValueError::new_err(
                        "SetSilencer expects a FixedCompletionTime or FixedUpdateRate",
                    ));
                }
            }
        };
        Ok(Self(Arc::new(inner)))
    }

    #[staticmethod]
    fn disable() -> Self {
        Self(Arc::new(SetSilencer::disable()))
    }
}

fn non_zero_u32(field: &str, value: u32) -> PyResult<NonZeroU32> {
    NonZeroU32::new(value).ok_or_else(|| PyValueError::new_err(format!("{field} must be >= 1")))
}

#[pyclass(
    name = "PtpConfig",
    module = "autd3.commands",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct PtpConfigPy(CorePtpConfig);

impl core::hash::Hash for PtpConfigPy {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        format!("{:?}", self.0).hash(state);
    }
}

#[pymethods]
impl PtpConfigPy {
    #[new]
    #[pyo3(signature = (
        *,
        sync_interval = CorePtpConfig::default().sync_interval,
        tx_timestamp_timeout = CorePtpConfig::default().tx_timestamp_timeout,
        delay_resp_timeout = CorePtpConfig::default().delay_resp_timeout,
        holdover = CorePtpConfig::default().holdover,
        lock_samples = CorePtpConfig::default().lock_samples.get(),
        step_threshold = CorePtpConfig::default().step_threshold,
        lock_threshold = CorePtpConfig::default().lock_threshold,
        kp_milli = CorePtpConfig::default().kp_milli,
        ki_milli = CorePtpConfig::default().ki_milli,
        max_freq_ppb = CorePtpConfig::default().max_freq_ppb,
        delay_req_syncs = CorePtpConfig::default().delay_req_syncs,
        path_delay_filter_shift = CorePtpConfig::default().path_delay_filter_shift,
        pause_quanta = CorePtpConfig::default().pause_quanta,
        pause_hold_syncs = CorePtpConfig::default().pause_hold_syncs,
        pause_retry = CorePtpConfig::default().pause_retry,
    ))]
    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    fn new(
        #[pyo3(from_py_with = extract_duration)] sync_interval: Duration,
        #[pyo3(from_py_with = extract_duration)] tx_timestamp_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] delay_resp_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] holdover: Duration,
        lock_samples: u16,
        #[pyo3(from_py_with = extract_duration)] step_threshold: Duration,
        #[pyo3(from_py_with = extract_duration)] lock_threshold: Duration,
        kp_milli: u32,
        ki_milli: u32,
        max_freq_ppb: u32,
        delay_req_syncs: NonZeroU16,
        path_delay_filter_shift: u8,
        pause_quanta: Option<NonZeroU16>,
        pause_hold_syncs: u16,
        #[pyo3(from_py_with = extract_duration)] pause_retry: Duration,
    ) -> PyResult<Self> {
        Ok(Self(CorePtpConfig {
            sync_interval,
            tx_timestamp_timeout,
            delay_resp_timeout,
            holdover,
            lock_samples: NonZeroU16::new(lock_samples)
                .ok_or_else(|| PyValueError::new_err("lock_samples must be >= 1"))?,
            step_threshold,
            lock_threshold,
            kp_milli,
            ki_milli,
            max_freq_ppb,
            delay_req_syncs,
            path_delay_filter_shift,
            pause_quanta,
            pause_hold_syncs,
            pause_retry,
        }))
    }

    #[getter]
    fn sync_interval<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.sync_interval)
    }

    #[getter]
    fn tx_timestamp_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.tx_timestamp_timeout)
    }

    #[getter]
    fn delay_resp_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.delay_resp_timeout)
    }

    #[getter]
    fn holdover<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.holdover)
    }

    #[getter]
    fn lock_samples(&self) -> u16 {
        self.0.lock_samples.get()
    }

    #[getter]
    fn step_threshold<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.step_threshold)
    }

    #[getter]
    fn lock_threshold<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.lock_threshold)
    }

    #[getter]
    fn kp_milli(&self) -> u32 {
        self.0.kp_milli
    }

    #[getter]
    fn ki_milli(&self) -> u32 {
        self.0.ki_milli
    }

    #[getter]
    fn max_freq_ppb(&self) -> u32 {
        self.0.max_freq_ppb
    }

    #[getter]
    fn delay_req_syncs(&self) -> NonZeroU16 {
        self.0.delay_req_syncs
    }

    #[getter]
    fn path_delay_filter_shift(&self) -> u8 {
        self.0.path_delay_filter_shift
    }

    #[getter]
    fn pause_quanta(&self) -> Option<NonZeroU16> {
        self.0.pause_quanta
    }

    #[getter]
    fn pause_hold_syncs(&self) -> u16 {
        self.0.pause_hold_syncs
    }

    #[getter]
    fn pause_retry<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.pause_retry)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(
    name = "FpgaBusWait",
    module = "autd3.commands",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FpgaBusWaitPy(CoreFpgaBusWait);

impl core::hash::Hash for FpgaBusWaitPy {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        format!("{:?}", self.0).hash(state);
    }
}

#[pymethods]
impl FpgaBusWaitPy {
    #[classattr]
    #[pyo3(name = "Cycles2")]
    fn cycles2() -> Self {
        Self(CoreFpgaBusWait::Cycles2)
    }

    #[classattr]
    #[pyo3(name = "Cycles3")]
    fn cycles3() -> Self {
        Self(CoreFpgaBusWait::Cycles3)
    }

    #[getter]
    fn cycles(&self) -> u8 {
        self.0.as_u8()
    }

    fn __repr__(&self) -> String {
        format!("FpgaBusWait.{:?}", self.0)
    }
}

#[pyclass(
    name = "CpuConfig",
    module = "autd3.commands",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuConfigPy(CoreCpuConfig);

impl core::hash::Hash for CpuConfigPy {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        format!("{:?}", self.0).hash(state);
    }
}

#[pymethods]
impl CpuConfigPy {
    #[new]
    #[pyo3(signature = (
        *,
        sys_time_transition_margin = CoreCpuConfig::default().sys_time_transition_margin,
        fpga_wait_update_max_polls = CoreCpuConfig::default().fpga_wait_update_max_polls.get(),
        fpga_flash_max_polls = CoreCpuConfig::default().fpga_flash_max_polls.get(),
        sync_guard = CoreCpuConfig::default().sync_guard,
        update_activate_delay = CoreCpuConfig::default().update_activate_delay,
        failsafe_timeout = CoreCpuConfig::default().failsafe_timeout,
        ptp_unlock_failsafe_timeout = CoreCpuConfig::default().ptp_unlock_failsafe_timeout,
        fpga_bus_wait = FpgaBusWaitPy(CoreCpuConfig::default().fpga_bus_wait),
        ptp = PtpConfigPy::default(),
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        #[pyo3(from_py_with = extract_duration)] sys_time_transition_margin: Duration,
        fpga_wait_update_max_polls: u32,
        fpga_flash_max_polls: u32,
        #[pyo3(from_py_with = extract_duration)] sync_guard: Duration,
        #[pyo3(from_py_with = extract_duration)] update_activate_delay: Duration,
        #[pyo3(from_py_with = crate::udp::none_or_duration)] failsafe_timeout: Option<Duration>,
        #[pyo3(from_py_with = crate::udp::none_or_duration)] ptp_unlock_failsafe_timeout: Option<
            Duration,
        >,
        fpga_bus_wait: FpgaBusWaitPy,
        ptp: PtpConfigPy,
    ) -> PyResult<Self> {
        Ok(Self(CoreCpuConfig {
            sys_time_transition_margin,
            fpga_wait_update_max_polls: non_zero_u32(
                "fpga_wait_update_max_polls",
                fpga_wait_update_max_polls,
            )?,
            fpga_flash_max_polls: non_zero_u32("fpga_flash_max_polls", fpga_flash_max_polls)?,
            sync_guard,
            update_activate_delay,
            failsafe_timeout,
            ptp_unlock_failsafe_timeout,
            fpga_bus_wait: fpga_bus_wait.0,
            ptp: ptp.0,
        }))
    }

    #[getter]
    fn sys_time_transition_margin<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.sys_time_transition_margin)
    }

    #[getter]
    fn fpga_wait_update_max_polls(&self) -> u32 {
        self.0.fpga_wait_update_max_polls.get()
    }

    #[getter]
    fn fpga_flash_max_polls(&self) -> u32 {
        self.0.fpga_flash_max_polls.get()
    }

    #[getter]
    fn sync_guard<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.sync_guard)
    }

    #[getter]
    fn update_activate_delay<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.update_activate_delay)
    }

    #[getter]
    fn failsafe_timeout<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.0
            .failsafe_timeout
            .map(|timeout| duration_to_py(py, timeout))
            .transpose()
    }

    #[getter]
    fn ptp_unlock_failsafe_timeout<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.0
            .ptp_unlock_failsafe_timeout
            .map(|timeout| duration_to_py(py, timeout))
            .transpose()
    }

    #[getter]
    fn fpga_bus_wait(&self) -> FpgaBusWaitPy {
        FpgaBusWaitPy(self.0.fpga_bus_wait)
    }

    #[getter]
    fn ptp(&self) -> PtpConfigPy {
        PtpConfigPy(self.0.ptp)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "SetCpuConfig", module = "autd3.commands", frozen)]
pub struct SetCpuConfigPy(pub(crate) Arc<CoreSetCpuConfig>);

#[pymethods]
impl SetCpuConfigPy {
    #[new]
    #[pyo3(signature = (config = CpuConfigPy::default()))]
    fn new(config: CpuConfigPy) -> Self {
        Self(Arc::new(CoreSetCpuConfig::new(config.0)))
    }

    #[getter]
    fn config(&self) -> CpuConfigPy {
        CpuConfigPy(self.0.config)
    }
}

#[pyclass(
    name = "GpioOut",
    module = "autd3.commands",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GpioOut(pub(crate) CoreGpioOut);

impl core::hash::Hash for GpioOut {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        format!("{:?}", self.0).hash(state);
    }
}

#[pymethods]
impl GpioOut {
    #[classattr]
    #[pyo3(name = "Off")]
    fn off() -> Self {
        Self(CoreGpioOut::Off)
    }

    #[classattr]
    #[pyo3(name = "BaseSignal")]
    fn base_signal() -> Self {
        Self(CoreGpioOut::BaseSignal)
    }

    #[classattr]
    #[pyo3(name = "Thermo")]
    fn thermo() -> Self {
        Self(CoreGpioOut::Thermo)
    }

    #[classattr]
    #[pyo3(name = "ForceFan")]
    fn force_fan() -> Self {
        Self(CoreGpioOut::ForceFan)
    }

    #[classattr]
    #[pyo3(name = "Sync")]
    fn sync() -> Self {
        Self(CoreGpioOut::Sync)
    }

    #[classattr]
    #[pyo3(name = "ModBank")]
    fn mod_bank() -> Self {
        Self(CoreGpioOut::ModBank)
    }

    #[classattr]
    #[pyo3(name = "PatternBank")]
    fn pattern_bank() -> Self {
        Self(CoreGpioOut::PatternBank)
    }

    #[classattr]
    #[pyo3(name = "IsStmMode")]
    fn is_stm_mode() -> Self {
        Self(CoreGpioOut::IsStmMode)
    }

    #[classattr]
    #[pyo3(name = "SyncDiff")]
    fn sync_diff() -> Self {
        Self(CoreGpioOut::SyncDiff)
    }

    #[staticmethod]
    #[pyo3(name = "ModIdx")]
    fn mod_idx(idx: u16) -> Self {
        Self(CoreGpioOut::ModIdx(idx))
    }

    #[staticmethod]
    #[pyo3(name = "PatternIdx")]
    fn pattern_idx(idx: u16) -> Self {
        Self(CoreGpioOut::PatternIdx(idx))
    }

    #[staticmethod]
    #[pyo3(name = "SysTimeEq")]
    fn sys_time_eq(sys_time: SysTime) -> Self {
        Self(CoreGpioOut::SysTimeEq(sys_time.0))
    }

    #[staticmethod]
    #[pyo3(name = "PwmOut")]
    fn pwm_out(transducer: u8) -> Self {
        Self(CoreGpioOut::PwmOut(transducer))
    }

    #[staticmethod]
    #[pyo3(name = "Direct")]
    fn direct(on: bool) -> Self {
        Self(CoreGpioOut::Direct(on))
    }

    fn __repr__(&self) -> String {
        format!("GpioOut.{:?}", self.0)
    }
}

#[pyclass(name = "SetGpioOut", module = "autd3.commands", frozen)]
pub struct SetGpioOutPy(pub(crate) Arc<SetGpioOut>);

#[pymethods]
impl SetGpioOutPy {
    #[new]
    fn new(outputs: Vec<GpioOut>) -> PyResult<Self> {
        let outputs: [GpioOut; 4] = outputs
            .try_into()
            .map_err(|_| PyValueError::new_err("SetGpioOut needs exactly 4 outputs"))?;
        Ok(Self(Arc::new(SetGpioOut {
            outputs: outputs.map(|g| g.0),
        })))
    }
}

#[pyclass(name = "EmulateGpioIn", module = "autd3.commands", frozen)]
pub struct EmulateGpioInPy(pub(crate) Arc<EmulateGpioIn>);

#[pymethods]
impl EmulateGpioInPy {
    #[new]
    fn new(values: Vec<bool>) -> PyResult<Self> {
        let values: [bool; 4] = values
            .try_into()
            .map_err(|_| PyValueError::new_err("EmulateGpioIn needs exactly 4 values"))?;
        Ok(Self(Arc::new(EmulateGpioIn { values })))
    }
}

pub(crate) struct SetOutputMaskCmd {
    masks: Vec<Vec<bool>>,
}

impl PushCommand for SetOutputMaskCmd {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(SetOutputMask {
            masks: self.masks.as_slice(),
        })?;
        Ok(())
    }
}

#[pyclass(name = "SetOutputMask", module = "autd3.commands", frozen)]
pub struct SetOutputMaskPy(pub(crate) Arc<SetOutputMaskCmd>);

#[pymethods]
impl SetOutputMaskPy {
    #[new]
    fn new(masks: Vec<Vec<bool>>) -> PyResult<Self> {
        for device in &masks {
            if device.len() != Autd3::NUM_TRANSDUCERS {
                return Err(PyValueError::new_err(format!(
                    "each device mask needs {} entries, got {}",
                    Autd3::NUM_TRANSDUCERS,
                    device.len()
                )));
            }
        }
        Ok(Self(Arc::new(SetOutputMaskCmd { masks })))
    }
}

pub(crate) struct SetPhaseCorrectionCmd {
    phases: Vec<Vec<Phase>>,
}

impl PushCommand for SetPhaseCorrectionCmd {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(SetPhaseCorrection {
            phases: self.phases.as_slice(),
        })?;
        Ok(())
    }
}

#[pyclass(name = "SetPhaseCorrection", module = "autd3.commands", frozen)]
pub struct SetPhaseCorrectionPy(pub(crate) Arc<SetPhaseCorrectionCmd>);

#[pymethods]
impl SetPhaseCorrectionPy {
    #[new]
    fn new(phases: Vec<Vec<Bound<'_, PyAny>>>) -> PyResult<Self> {
        let phases = phases
            .into_iter()
            .map(|device| {
                if device.len() != Autd3::NUM_TRANSDUCERS {
                    return Err(PyValueError::new_err(format!(
                        "each device needs {} phases, got {}",
                        Autd3::NUM_TRANSDUCERS,
                        device.len()
                    )));
                }
                device
                    .iter()
                    .map(|p| extract_u8(p).map(Phase))
                    .collect::<PyResult<Vec<_>>>()
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self(Arc::new(SetPhaseCorrectionCmd { phases })))
    }
}

pub(crate) struct SetPulseWidthTableCmd {
    table: [CorePulseWidth; PWE_TABLE_SIZE],
}

impl PushCommand for SetPulseWidthTableCmd {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(CoreSetPulseWidthTable { table: &self.table })?;
        Ok(())
    }
}

#[pyclass(name = "SetPulseWidthTable", module = "autd3.commands", frozen)]
pub struct SetPulseWidthTablePy(pub(crate) Arc<SetPulseWidthTableCmd>);

#[pymethods]
impl SetPulseWidthTablePy {
    #[new]
    #[pyo3(signature = (table=None))]
    fn new(table: Option<Vec<PulseWidth>>) -> PyResult<Self> {
        let Some(table) = table else {
            return Ok(Self(Arc::new(SetPulseWidthTableCmd {
                table: *CoreSetPulseWidthTable::default().table,
            })));
        };
        let table: [PulseWidth; PWE_TABLE_SIZE] =
            table.try_into().map_err(|v: Vec<PulseWidth>| {
                PyValueError::new_err(format!(
                    "SetPulseWidthTable needs exactly {PWE_TABLE_SIZE} entries, got {}",
                    v.len()
                ))
            })?;
        Ok(Self(Arc::new(SetPulseWidthTableCmd {
            table: table.map(|pulse_width| pulse_width.0),
        })))
    }

    #[staticmethod]
    fn empty_table() -> Vec<PulseWidth> {
        CoreSetPulseWidthTable::empty_table()
            .into_iter()
            .map(PulseWidth)
            .collect()
    }
}

#[pyclass(
    name = "PulseWidth",
    module = "autd3.value",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq)]
pub struct PulseWidth(pub(crate) CorePulseWidth);

impl core::hash::Hash for PulseWidth {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.pulse_width().ok().hash(state);
    }
}

#[pymethods]
impl PulseWidth {
    #[new]
    fn new(pulse_width: u16) -> Self {
        Self(CorePulseWidth::new(pulse_width))
    }

    #[staticmethod]
    fn from_duty(duty: f32) -> Self {
        Self(CorePulseWidth::from_duty(duty))
    }

    fn pulse_width(&self) -> PyResult<u16> {
        self.0
            .pulse_width()
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Clear>()?;
    m.add_class::<Synchronize>()?;
    m.add_class::<ReleaseFailsafe>()?;
    m.add_class::<Nop>()?;
    m.add_class::<ForceFan>()?;
    m.add_class::<FixedCompletionTimePy>()?;
    m.add_class::<FixedUpdateRatePy>()?;
    m.add_class::<SetSilencerPy>()?;
    m.add_class::<FpgaBusWaitPy>()?;
    m.add_class::<PtpConfigPy>()?;
    m.add_class::<CpuConfigPy>()?;
    m.add_class::<SetCpuConfigPy>()?;
    m.add_class::<GpioOut>()?;
    m.add_class::<SetGpioOutPy>()?;
    m.add_class::<EmulateGpioInPy>()?;
    m.add_class::<SetOutputMaskPy>()?;
    m.add_class::<SetPhaseCorrectionPy>()?;
    m.add_class::<SetPulseWidthTablePy>()?;
    m.add_class::<PulseWidth>()?;
    Ok(())
}
