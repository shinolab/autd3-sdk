use core::num::NonZeroU16;

use std::sync::Arc;

use autd3_python_capsule::extract::{
    duration_to_py, extract_duration, extract_sampling_config, extract_velocity,
};
use autd3_python_capsule::{capsule_of, modulation_from_capsule};
use autd3_rs::commands::Expansion;
use autd3_rs::commands::{
    ActivateModulationBank as CoreActivateModulationBank,
    ActivatePatternBank as CoreActivatePatternBank, ConfigFociStm as CoreConfigFociStm,
    ConfigModulation as CoreConfigModulation, ConfigPattern as CoreConfigPattern,
    PhaseDepth as CorePhaseDepth, WriteModulationBuffer as CoreWriteModulationBuffer,
    WritePatternBuffer as CoreWritePatternBuffer, WritePatternPhase as CoreWritePatternPhase,
};
use autd3_rs::value::{
    GpioIn as CoreGpioIn, Intensity, LoopBehavior as CoreLoopBehavior,
    ModulationBank as CoreModulationBank, PatternBank as CorePatternBank, Phase,
    SysTime as CoreSysTime, TransitionMode as CoreTransitionMode,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::datagram::{OwnedPatternIntensity, PushCommand};

#[pyclass(
    name = "PatternBank",
    module = "autd3.value",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PatternBank(pub(crate) CorePatternBank);

impl core::hash::Hash for PatternBank {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
    }
}

#[pymethods]
impl PatternBank {
    #[classattr]
    #[pyo3(name = "B0")]
    fn b0() -> Self {
        Self(CorePatternBank::B0)
    }

    #[classattr]
    #[pyo3(name = "B1")]
    fn b1() -> Self {
        Self(CorePatternBank::B1)
    }

    fn __repr__(&self) -> String {
        format!("PatternBank.{:?}", self.0)
    }
}

#[pyclass(
    name = "ModulationBank",
    module = "autd3.value",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ModulationBank(pub(crate) CoreModulationBank);

impl core::hash::Hash for ModulationBank {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
    }
}

#[pymethods]
impl ModulationBank {
    #[classattr]
    #[pyo3(name = "B0")]
    fn b0() -> Self {
        Self(CoreModulationBank::B0)
    }

    #[classattr]
    #[pyo3(name = "B1")]
    fn b1() -> Self {
        Self(CoreModulationBank::B1)
    }

    fn __repr__(&self) -> String {
        format!("ModulationBank.{:?}", self.0)
    }
}

#[pyclass(
    name = "GpioIn",
    module = "autd3.value",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpioIn(pub(crate) CoreGpioIn);

#[pymethods]
impl GpioIn {
    #[classattr]
    #[pyo3(name = "I0")]
    fn i0() -> Self {
        Self(CoreGpioIn::I0)
    }

    #[classattr]
    #[pyo3(name = "I1")]
    fn i1() -> Self {
        Self(CoreGpioIn::I1)
    }

    #[classattr]
    #[pyo3(name = "I2")]
    fn i2() -> Self {
        Self(CoreGpioIn::I2)
    }

    #[classattr]
    #[pyo3(name = "I3")]
    fn i3() -> Self {
        Self(CoreGpioIn::I3)
    }

    fn __repr__(&self) -> String {
        format!("GpioIn.{:?}", self.0)
    }
}

#[pyclass(
    name = "SysTime",
    module = "autd3.value",
    eq,
    ord,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SysTime(pub(crate) CoreSysTime);

#[pymethods]
impl SysTime {
    #[classattr]
    #[pyo3(name = "ZERO")]
    fn zero() -> Self {
        Self(CoreSysTime::ZERO)
    }

    #[staticmethod]
    #[pyo3(name = "from_nanos")]
    fn from_nanos(sys_time_ns: u64) -> Self {
        Self(CoreSysTime::from_nanos(sys_time_ns))
    }

    #[getter]
    fn sys_time(&self) -> u64 {
        self.0.sys_time()
    }

    fn __add__(&self, duration: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0 + extract_duration(duration)?))
    }

    fn __sub__<'py>(&self, rhs: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = rhs.py();
        if let Ok(other) = rhs.cast::<Self>() {
            return duration_to_py(py, self.0 - other.get().0);
        }
        Ok(Bound::new(py, Self(self.0 - extract_duration(rhs)?))?.into_any())
    }

    fn __repr__(&self) -> String {
        format!("SysTime.from_nanos({})", self.0.sys_time())
    }
}

#[pyclass(
    name = "TransitionMode",
    module = "autd3.value",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransitionMode(pub(crate) CoreTransitionMode);

#[pymethods]
impl TransitionMode {
    #[classattr]
    #[pyo3(name = "SyncIdx")]
    fn sync_idx() -> Self {
        Self(CoreTransitionMode::SyncIdx)
    }

    #[classattr]
    #[pyo3(name = "Ext")]
    fn ext() -> Self {
        Self(CoreTransitionMode::Ext)
    }

    #[classattr]
    #[pyo3(name = "Immediate")]
    fn immediate() -> Self {
        Self(CoreTransitionMode::Immediate)
    }

    #[classattr]
    #[pyo3(name = "Later")]
    fn later() -> Self {
        Self(CoreTransitionMode::Later)
    }

    #[staticmethod]
    #[pyo3(name = "SysTime")]
    fn sys_time(sys_time: SysTime) -> Self {
        Self(CoreTransitionMode::SysTime { time: sys_time.0 })
    }

    #[staticmethod]
    #[pyo3(name = "Gpio")]
    fn gpio(gpio: GpioIn) -> Self {
        Self(CoreTransitionMode::Gpio(gpio.0))
    }

    fn is_later(&self) -> bool {
        self.0.is_later()
    }

    fn __repr__(&self) -> String {
        format!("TransitionMode.{:?}", self.0)
    }
}

#[pyclass(
    name = "LoopBehavior",
    module = "autd3.value",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoopBehavior(pub(crate) CoreLoopBehavior);

#[pymethods]
impl LoopBehavior {
    #[classattr]
    #[pyo3(name = "Infinite")]
    fn infinite() -> Self {
        Self(CoreLoopBehavior::Infinite)
    }

    #[classattr]
    #[pyo3(name = "Once")]
    fn once() -> Self {
        Self(CoreLoopBehavior::ONCE)
    }

    #[staticmethod]
    #[pyo3(name = "Finite")]
    fn finite(count: u16) -> PyResult<Self> {
        let count = NonZeroU16::new(count)
            .ok_or_else(|| PyValueError::new_err("loop count must be >= 1"))?;
        Ok(Self(CoreLoopBehavior::Finite(count)))
    }

    fn rep(&self) -> u16 {
        self.0.rep()
    }

    fn __repr__(&self) -> String {
        format!("LoopBehavior.{:?}", self.0)
    }
}

pub(crate) struct WritePatternBufferData {
    bank: CorePatternBank,
    index: usize,
    phases: Vec<Vec<Phase>>,
    intensities: OwnedPatternIntensity,
}

impl PushCommand for WritePatternBufferData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(CoreWritePatternBuffer::new(
            self.bank,
            self.index,
            &self.phases,
            self.intensities.as_ref(),
        ))?;
        Ok(())
    }
}

#[pyclass(name = "WritePatternBuffer", module = "autd3.commands", frozen)]
pub struct WritePatternBuffer(pub(crate) Arc<WritePatternBufferData>);

#[pymethods]
impl WritePatternBuffer {
    #[new]
    fn new(
        bank: PatternBank,
        index: u16,
        phases: &Bound<'_, PyAny>,
        intensities: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self(Arc::new(WritePatternBufferData {
            bank: bank.0,
            index: usize::from(index),
            phases: crate::datagram::extract_phases(phases)?,
            intensities: crate::datagram::extract_pattern_intensity(intensities)?,
        })))
    }
}

#[pyclass(
    name = "PhaseDepth",
    module = "autd3.commands",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PhaseDepth(pub(crate) CorePhaseDepth);

impl core::hash::Hash for PhaseDepth {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
    }
}

#[pymethods]
impl PhaseDepth {
    #[classattr]
    #[pyo3(name = "Bits8")]
    fn bits8() -> Self {
        Self(CorePhaseDepth::Bits8)
    }

    #[classattr]
    #[pyo3(name = "Bits4")]
    fn bits4() -> Self {
        Self(CorePhaseDepth::Bits4)
    }

    fn max_count(&self) -> usize {
        self.0.max_count()
    }

    fn __repr__(&self) -> String {
        format!("PhaseDepth.{:?}", self.0)
    }
}

pub(crate) struct WritePatternPhaseData {
    bank: CorePatternBank,
    index: usize,
    depth: CorePhaseDepth,
    intensity: Intensity,
    patterns: Vec<Vec<Vec<Phase>>>,
}

impl PushCommand for WritePatternPhaseData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(CoreWritePatternPhase {
            bank: self.bank,
            index: self.index,
            depth: self.depth,
            intensity: self.intensity,
            patterns: &self.patterns,
        })?;
        Ok(())
    }
}

#[pyclass(name = "WritePatternPhase", module = "autd3.commands", frozen)]
pub struct WritePatternPhase(pub(crate) Arc<WritePatternPhaseData>);

#[pymethods]
impl WritePatternPhase {
    #[new]
    fn new(
        bank: PatternBank,
        index: u16,
        depth: PhaseDepth,
        intensity: u8,
        patterns: Vec<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let patterns = patterns
            .iter()
            .map(crate::datagram::extract_phases)
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self(Arc::new(WritePatternPhaseData {
            bank: bank.0,
            index: usize::from(index),
            depth: depth.0,
            intensity: Intensity(intensity),
            patterns,
        })))
    }
}

#[pyclass(name = "ConfigPattern", module = "autd3.commands", frozen)]
pub struct ConfigPattern(pub(crate) Arc<CoreConfigPattern>);

#[pymethods]
impl ConfigPattern {
    #[new]
    #[pyo3(signature = (bank, config, size, loop_behavior = None))]
    fn new(
        bank: PatternBank,
        config: &Bound<'_, PyAny>,
        size: usize,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        Ok(Self(Arc::new(CoreConfigPattern {
            bank: bank.0,
            config: extract_sampling_config(config)?,
            size,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })))
    }
}

#[pyclass(name = "ConfigFociStm", module = "autd3.commands", frozen)]
pub struct ConfigFociStm(pub(crate) Arc<CoreConfigFociStm>);

#[pymethods]
impl ConfigFociStm {
    #[new]
    #[pyo3(signature = (bank, config, size, num_foci, sound_speed, loop_behavior = None))]
    fn new(
        bank: PatternBank,
        config: &Bound<'_, PyAny>,
        size: usize,
        num_foci: u8,
        sound_speed: &Bound<'_, PyAny>,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        let config = extract_sampling_config(config)?;
        let sound_speed = extract_velocity(sound_speed)?;
        Ok(Self(Arc::new(CoreConfigFociStm {
            bank: bank.0,
            config,
            size,
            num_foci,
            sound_speed,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })))
    }
}

#[pyclass(name = "ActivatePatternBank", module = "autd3.commands", frozen)]
pub struct ActivatePatternBank(pub(crate) Arc<CoreActivatePatternBank>);

#[pymethods]
impl ActivatePatternBank {
    #[new]
    #[pyo3(signature = (bank, transition_mode = None))]
    fn new(bank: PatternBank, transition_mode: Option<TransitionMode>) -> Self {
        Self(Arc::new(CoreActivatePatternBank {
            bank: bank.0,
            transition_mode: transition_mode.map_or(CoreTransitionMode::default(), |t| t.0),
        }))
    }
}

pub(crate) struct WriteModulationBufferData {
    bank: CoreModulationBank,
    offset: usize,
    data: Vec<u8>,
}

impl PushCommand for WriteModulationBufferData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(CoreWriteModulationBuffer {
            bank: self.bank,
            offset: self.offset,
            data: &self.data,
        })?;
        Ok(())
    }
}

#[pyclass(name = "WriteModulationBuffer", module = "autd3.commands", frozen)]
pub struct WriteModulationBuffer(pub(crate) Arc<WriteModulationBufferData>);

#[pymethods]
impl WriteModulationBuffer {
    #[new]
    fn new(bank: ModulationBank, offset: usize, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        let capsule = capsule_of(data)?;
        let data = modulation_from_capsule(&capsule)?.to_vec();
        Ok(Self(Arc::new(WriteModulationBufferData {
            bank: bank.0,
            offset,
            data,
        })))
    }
}

#[pyclass(name = "ConfigModulation", module = "autd3.commands", frozen)]
pub struct ConfigModulation(pub(crate) Arc<CoreConfigModulation>);

#[pymethods]
impl ConfigModulation {
    #[new]
    #[pyo3(signature = (bank, config, size, loop_behavior = None))]
    fn new(
        bank: ModulationBank,
        config: &Bound<'_, PyAny>,
        size: usize,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        Ok(Self(Arc::new(CoreConfigModulation {
            bank: bank.0,
            config: extract_sampling_config(config)?,
            size,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })))
    }
}

#[pyclass(name = "ActivateModulationBank", module = "autd3.commands", frozen)]
pub struct ActivateModulationBank(pub(crate) Arc<CoreActivateModulationBank>);

#[pymethods]
impl ActivateModulationBank {
    #[new]
    #[pyo3(signature = (bank, transition_mode = None))]
    fn new(bank: ModulationBank, transition_mode: Option<TransitionMode>) -> Self {
        Self(Arc::new(CoreActivateModulationBank {
            bank: bank.0,
            transition_mode: transition_mode.map_or(CoreTransitionMode::default(), |t| t.0),
        }))
    }
}

#[pyclass(
    name = "Telemetry",
    module = "autd3.value",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Telemetry(pub(crate) autd3_rs::Telemetry);

impl core::hash::Hash for Telemetry {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
    }
}

#[pymethods]
impl Telemetry {
    #[classattr]
    #[pyo3(name = "FifoDrop")]
    fn fifo_drop() -> Self {
        Self(autd3_rs::Telemetry::FifoDrop)
    }

    #[classattr]
    #[pyo3(name = "Dedup")]
    fn dedup() -> Self {
        Self(autd3_rs::Telemetry::Dedup)
    }

    #[classattr]
    #[pyo3(name = "SeqMismatch")]
    fn seq_mismatch() -> Self {
        Self(autd3_rs::Telemetry::SeqMismatch)
    }

    #[classattr]
    #[pyo3(name = "DispatchError")]
    fn dispatch_error() -> Self {
        Self(autd3_rs::Telemetry::DispatchError)
    }

    #[classattr]
    #[pyo3(name = "Processed")]
    fn processed() -> Self {
        Self(autd3_rs::Telemetry::Processed)
    }

    #[classattr]
    #[pyo3(name = "Failsafe")]
    fn failsafe() -> Self {
        Self(autd3_rs::Telemetry::Failsafe)
    }

    #[classattr]
    #[pyo3(name = "SyncResync")]
    fn sync_resync() -> Self {
        Self(autd3_rs::Telemetry::SyncResync)
    }

    #[classattr]
    #[pyo3(name = "PtpUnlockFailsafe")]
    fn ptp_unlock_failsafe() -> Self {
        Self(autd3_rs::Telemetry::PtpUnlockFailsafe)
    }

    #[classattr]
    #[pyo3(name = "SendFailure")]
    fn send_failure() -> Self {
        Self(autd3_rs::Telemetry::SendFailure)
    }

    #[classattr]
    #[pyo3(name = "BootFailure")]
    fn boot_failure() -> Self {
        Self(autd3_rs::Telemetry::BootFailure)
    }

    #[classattr]
    #[pyo3(name = "ALL")]
    fn all(py: Python<'_>) -> PyResult<Bound<'_, pyo3::types::PyTuple>> {
        pyo3::types::PyTuple::new(py, autd3_rs::Telemetry::ALL.iter().copied().map(Self))
    }

    fn __repr__(&self) -> String {
        format!("Telemetry.{:?}", self.0)
    }
}
