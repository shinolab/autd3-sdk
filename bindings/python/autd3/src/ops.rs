use core::num::NonZeroU16;

use autd3_python_capsule::{
    DevicePattern, capsule_of, modulation_from_capsule, pattern_from_capsule,
};
use autd3_rs::Velocity;
use autd3_rs::commands::PatternCompression as CorePatternCompression;
use autd3_rs::value::{
    DcSysTime, GpioIn as CoreGpioIn, LoopBehavior as CoreLoopBehavior,
    ModulationBank as CoreModulationBank, PatternBank as CorePatternBank,
    TransitionMode as CoreTransitionMode,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "PatternBank", module = "autd3.value", from_py_object)]
#[derive(Clone, Copy)]
pub struct PatternBank(pub(crate) CorePatternBank);

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

#[pyclass(name = "ModulationBank", module = "autd3.value", from_py_object)]
#[derive(Clone, Copy)]
pub struct ModulationBank(pub(crate) CoreModulationBank);

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

#[pyclass(name = "GpioIn", module = "autd3.value", from_py_object)]
#[derive(Clone, Copy)]
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

#[pyclass(name = "TransitionMode", module = "autd3.value", from_py_object)]
#[derive(Clone, Copy)]
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

    #[staticmethod]
    #[pyo3(name = "SysTime")]
    fn sys_time(sys_time_ns: u64) -> Self {
        Self(CoreTransitionMode::SysTime(DcSysTime::from_nanos(
            sys_time_ns,
        )))
    }

    #[staticmethod]
    #[pyo3(name = "Gpio")]
    fn gpio(gpio: GpioIn) -> Self {
        Self(CoreTransitionMode::Gpio(gpio.0))
    }

    fn __repr__(&self) -> String {
        format!("TransitionMode.{:?}", self.0)
    }
}

#[pyclass(name = "LoopBehavior", module = "autd3.value", from_py_object)]
#[derive(Clone, Copy)]
pub struct LoopBehavior(pub(crate) CoreLoopBehavior);

#[pymethods]
impl LoopBehavior {
    #[classattr]
    #[pyo3(name = "Infinite")]
    fn infinite() -> Self {
        Self(CoreLoopBehavior::Infinite)
    }

    #[classattr]
    #[pyo3(name = "ONCE")]
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

#[pyclass(name = "WritePatternBuffer", module = "autd3.commands")]
pub struct WritePatternBuffer {
    pub(crate) bank: CorePatternBank,
    pub(crate) index: u16,
    pub(crate) emissions: Vec<DevicePattern>,
}

#[pymethods]
impl WritePatternBuffer {
    #[new]
    fn new(bank: PatternBank, index: u16, emissions: &Bound<'_, PyAny>) -> PyResult<Self> {
        let capsule = capsule_of(emissions)?;
        let emissions = pattern_from_capsule(&capsule)?.to_vec();
        Ok(Self {
            bank: bank.0,
            index,
            emissions,
        })
    }
}

#[pyclass(name = "PatternCompression", module = "autd3.commands", from_py_object)]
#[derive(Clone, Copy)]
pub struct PatternCompression(pub(crate) CorePatternCompression);

#[pymethods]
impl PatternCompression {
    #[classattr]
    #[pyo3(name = "PhaseFull")]
    fn phase_full() -> Self {
        Self(CorePatternCompression::PhaseFull)
    }

    #[classattr]
    #[pyo3(name = "PhaseHalf")]
    fn phase_half() -> Self {
        Self(CorePatternCompression::PhaseHalf)
    }

    fn per_frame(&self) -> usize {
        self.0.per_frame()
    }

    fn __repr__(&self) -> String {
        format!("PatternCompression.{:?}", self.0)
    }
}

#[pyclass(name = "WritePatternCompressed", module = "autd3.commands")]
pub struct WritePatternCompressed {
    pub(crate) bank: CorePatternBank,
    pub(crate) index: u32,
    pub(crate) format: CorePatternCompression,
    pub(crate) patterns: Vec<Vec<DevicePattern>>,
}

#[pymethods]
impl WritePatternCompressed {
    #[new]
    fn new(
        bank: PatternBank,
        index: u32,
        format: PatternCompression,
        patterns: Vec<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        if patterns.is_empty() || patterns.len() > 4 {
            return Err(PyValueError::new_err(
                "WritePatternCompressed expects 1..=4 pattern buffers",
            ));
        }
        let patterns = patterns
            .iter()
            .map(|buffer| {
                let capsule = capsule_of(buffer)?;
                Ok(pattern_from_capsule(&capsule)?.to_vec())
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            bank: bank.0,
            index,
            format: format.0,
            patterns,
        })
    }
}

#[pyclass(name = "ConfigPattern", module = "autd3.commands")]
pub struct ConfigPattern {
    pub(crate) bank: CorePatternBank,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) loop_behavior: CoreLoopBehavior,
}

#[pymethods]
impl ConfigPattern {
    #[new]
    #[pyo3(signature = (bank, config, size, loop_behavior = None))]
    fn new(
        bank: PatternBank,
        config: &Bound<'_, PyAny>,
        size: u32,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        let divider = config.call_method0("divide")?.extract::<u16>()?;
        Ok(Self {
            bank: bank.0,
            divider,
            size,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })
    }
}

#[pyclass(name = "ConfigFociStm", module = "autd3.commands")]
pub struct ConfigFociStm {
    pub(crate) bank: CorePatternBank,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) num_foci: u8,
    pub(crate) sound_speed: Velocity,
    pub(crate) loop_behavior: CoreLoopBehavior,
}

#[pymethods]
impl ConfigFociStm {
    #[new]
    #[pyo3(signature = (bank, config, size, num_foci, sound_speed, loop_behavior = None))]
    fn new(
        bank: PatternBank,
        config: &Bound<'_, PyAny>,
        size: u32,
        num_foci: u8,
        sound_speed: &Bound<'_, PyAny>,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        let divider = config.call_method0("divide")?.extract::<u16>()?;
        let mm_per_s: f32 = sound_speed
            .getattr("mm_per_s")
            .and_then(|v| v.extract())
            .map_err(|_| {
                PyValueError::new_err(
                    "sound speed must be a Velocity, e.g. 340 * m / s (bare numbers are no longer accepted)",
                )
            })?;
        Ok(Self {
            bank: bank.0,
            divider,
            size,
            num_foci,
            sound_speed: Velocity::from_mm_s(mm_per_s),
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })
    }
}

#[pyclass(name = "ChangePatternBank", module = "autd3.commands")]
pub struct ChangePatternBank {
    pub(crate) bank: CorePatternBank,
    pub(crate) transition_mode: CoreTransitionMode,
}

#[pymethods]
impl ChangePatternBank {
    #[new]
    #[pyo3(signature = (bank, transition_mode = None))]
    fn new(bank: PatternBank, transition_mode: Option<TransitionMode>) -> Self {
        Self {
            bank: bank.0,
            transition_mode: transition_mode.map_or(CoreTransitionMode::default(), |t| t.0),
        }
    }
}

#[pyclass(name = "WriteModulationBuffer", module = "autd3.commands")]
pub struct WriteModulationBuffer {
    pub(crate) bank: CoreModulationBank,
    pub(crate) offset: u32,
    pub(crate) data: Vec<u8>,
}

#[pymethods]
impl WriteModulationBuffer {
    #[new]
    fn new(bank: ModulationBank, offset: u32, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        let capsule = capsule_of(data)?;
        let data = modulation_from_capsule(&capsule)?.to_vec();
        Ok(Self {
            bank: bank.0,
            offset,
            data,
        })
    }
}

#[pyclass(name = "ConfigModulation", module = "autd3.commands")]
pub struct ConfigModulation {
    pub(crate) bank: CoreModulationBank,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) loop_behavior: CoreLoopBehavior,
}

#[pymethods]
impl ConfigModulation {
    #[new]
    #[pyo3(signature = (bank, config, size, loop_behavior = None))]
    fn new(
        bank: ModulationBank,
        config: &Bound<'_, PyAny>,
        size: u32,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        let divider = config.call_method0("divide")?.extract::<u16>()?;
        Ok(Self {
            bank: bank.0,
            divider,
            size,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })
    }
}

#[pyclass(name = "ChangeModulationBank", module = "autd3.commands")]
pub struct ChangeModulationBank {
    pub(crate) bank: CoreModulationBank,
    pub(crate) transition_mode: CoreTransitionMode,
}

#[pymethods]
impl ChangeModulationBank {
    #[new]
    #[pyo3(signature = (bank, transition_mode = None))]
    fn new(bank: ModulationBank, transition_mode: Option<TransitionMode>) -> Self {
        Self {
            bank: bank.0,
            transition_mode: transition_mode.map_or(CoreTransitionMode::default(), |t| t.0),
        }
    }
}
