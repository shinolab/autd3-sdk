use core::num::NonZeroU16;

use autd3_python_capsule::{
    DevicePattern, capsule_of, modulation_from_capsule, pattern_from_capsule,
};
use autd3_rs::value::{
    DcSysTime, GpioIn as CoreGpioIn, LoopBehavior as CoreLoopBehavior,
    ModulationBank as CoreModulationBank, PatternBank as CorePatternBank,
    TransitionMode as CoreTransitionMode,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "PatternBank", module = "autd3", from_py_object)]
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

#[pyclass(name = "ModulationBank", module = "autd3", from_py_object)]
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

#[derive(Clone, Copy)]
pub(crate) enum PatternData {
    Raw,
    Foci { num_foci: u8, sound_speed: u16 },
}

#[pyclass(name = "PatternDataType", module = "autd3", from_py_object)]
#[derive(Clone, Copy)]
pub struct PatternDataType(pub(crate) PatternData);

#[pymethods]
impl PatternDataType {
    #[classattr]
    #[pyo3(name = "Raw")]
    fn raw() -> Self {
        Self(PatternData::Raw)
    }

    #[staticmethod]
    fn foci(num_foci: u8, sound_speed: u16) -> Self {
        Self(PatternData::Foci {
            num_foci,
            sound_speed,
        })
    }

    fn __repr__(&self) -> String {
        match self.0 {
            PatternData::Raw => "PatternDataType.Raw".to_string(),
            PatternData::Foci {
                num_foci,
                sound_speed,
            } => format!("PatternDataType.Foci(num_foci={num_foci}, sound_speed={sound_speed})"),
        }
    }
}

#[pyclass(name = "GpioIn", module = "autd3", from_py_object)]
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

#[pyclass(name = "TransitionMode", module = "autd3", from_py_object)]
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

#[pyclass(name = "LoopBehavior", module = "autd3", from_py_object)]
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

#[pyclass(name = "WritePatternBuffer", module = "autd3")]
pub struct WritePatternBuffer {
    pub(crate) bank: CorePatternBank,
    pub(crate) index: u16,
    pub(crate) emissions: Vec<DevicePattern>,
}

#[pymethods]
impl WritePatternBuffer {
    #[new]
    fn new(bank: PatternBank, index: u16, buffer: &Bound<'_, PyAny>) -> PyResult<Self> {
        let capsule = capsule_of(buffer)?;
        let emissions = pattern_from_capsule(&capsule)?.to_vec();
        Ok(Self {
            bank: bank.0,
            index,
            emissions,
        })
    }
}

#[pyclass(name = "ConfigPattern", module = "autd3")]
pub struct ConfigPattern {
    pub(crate) bank: CorePatternBank,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) data_type: PatternData,
    pub(crate) loop_behavior: CoreLoopBehavior,
}

#[pymethods]
impl ConfigPattern {
    #[new]
    #[pyo3(signature = (bank, sampling_config, size, data_type, loop_behavior = None))]
    fn new(
        bank: PatternBank,
        sampling_config: &Bound<'_, PyAny>,
        size: u32,
        data_type: PatternDataType,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        let divider = sampling_config.call_method0("divide")?.extract::<u16>()?;
        Ok(Self {
            bank: bank.0,
            divider,
            size,
            data_type: data_type.0,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })
    }
}

#[pyclass(name = "ChangePatternBank", module = "autd3")]
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

#[pyclass(name = "WriteModulationBuffer", module = "autd3")]
pub struct WriteModulationBuffer {
    pub(crate) bank: CoreModulationBank,
    pub(crate) offset: u32,
    pub(crate) data: Vec<u8>,
}

#[pymethods]
impl WriteModulationBuffer {
    #[new]
    fn new(bank: ModulationBank, offset: u32, buffer: &Bound<'_, PyAny>) -> PyResult<Self> {
        let capsule = capsule_of(buffer)?;
        let data = modulation_from_capsule(&capsule)?.to_vec();
        Ok(Self {
            bank: bank.0,
            offset,
            data,
        })
    }
}

#[pyclass(name = "ConfigModulation", module = "autd3")]
pub struct ConfigModulation {
    pub(crate) bank: CoreModulationBank,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) loop_behavior: CoreLoopBehavior,
}

#[pymethods]
impl ConfigModulation {
    #[new]
    #[pyo3(signature = (bank, sampling_config, size, loop_behavior = None))]
    fn new(
        bank: ModulationBank,
        sampling_config: &Bound<'_, PyAny>,
        size: u32,
        loop_behavior: Option<LoopBehavior>,
    ) -> PyResult<Self> {
        let divider = sampling_config.call_method0("divide")?.extract::<u16>()?;
        Ok(Self {
            bank: bank.0,
            divider,
            size,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
        })
    }
}

#[pyclass(name = "ChangeModulationBank", module = "autd3")]
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
