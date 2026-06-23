use autd3_python_capsule::{
    DevicePattern, capsule_of, modulation_from_capsule, pattern_from_capsule,
};
use autd3_rs::value::{
    ModulationBank as CoreModulationBank, PatternBank as CorePatternBank,
    PatternDataType as CorePatternDataType, TransitionMode as CoreTransitionMode,
};
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

#[pyclass(name = "PatternDataType", module = "autd3", from_py_object)]
#[derive(Clone, Copy)]
pub struct PatternDataType(pub(crate) CorePatternDataType);

#[pymethods]
impl PatternDataType {
    #[classattr]
    #[pyo3(name = "Raw")]
    fn raw() -> Self {
        Self(CorePatternDataType::Raw)
    }

    #[staticmethod]
    fn foci(num_foci: u8, sound_speed: u16) -> Self {
        Self(CorePatternDataType::Foci {
            num_foci,
            sound_speed,
        })
    }

    fn __repr__(&self) -> String {
        format!("PatternDataType.{:?}", self.0)
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
    #[pyo3(name = "SysTime")]
    fn sys_time() -> Self {
        Self(CoreTransitionMode::SysTime)
    }

    #[classattr]
    #[pyo3(name = "Gpio")]
    fn gpio() -> Self {
        Self(CoreTransitionMode::Gpio)
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

    fn __repr__(&self) -> String {
        format!("TransitionMode.{:?}", self.0)
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
    pub(crate) data_type: CorePatternDataType,
}

#[pymethods]
impl ConfigPattern {
    #[new]
    fn new(bank: PatternBank, divider: u16, size: u32, data_type: PatternDataType) -> Self {
        Self {
            bank: bank.0,
            divider,
            size,
            data_type: data_type.0,
        }
    }
}

#[pyclass(name = "ChangePatternBank", module = "autd3")]
pub struct ChangePatternBank {
    pub(crate) bank: CorePatternBank,
    pub(crate) transition_mode: CoreTransitionMode,
    pub(crate) transition_value: u64,
}

#[pymethods]
impl ChangePatternBank {
    #[new]
    #[pyo3(signature = (bank, transition_mode = None, transition_value = 0))]
    fn new(
        bank: PatternBank,
        transition_mode: Option<TransitionMode>,
        transition_value: u64,
    ) -> Self {
        Self {
            bank: bank.0,
            transition_mode: transition_mode.map_or(CoreTransitionMode::default(), |t| t.0),
            transition_value,
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
}

#[pymethods]
impl ConfigModulation {
    #[new]
    fn new(bank: ModulationBank, divider: u16, size: u32) -> Self {
        Self {
            bank: bank.0,
            divider,
            size,
        }
    }
}

#[pyclass(name = "ChangeModulationBank", module = "autd3")]
pub struct ChangeModulationBank {
    pub(crate) bank: CoreModulationBank,
    pub(crate) transition_mode: CoreTransitionMode,
    pub(crate) transition_value: u64,
}

#[pymethods]
impl ChangeModulationBank {
    #[new]
    #[pyo3(signature = (bank, transition_mode = None, transition_value = 0))]
    fn new(
        bank: ModulationBank,
        transition_mode: Option<TransitionMode>,
        transition_value: u64,
    ) -> Self {
        Self {
            bank: bank.0,
            transition_mode: transition_mode.map_or(CoreTransitionMode::default(), |t| t.0),
            transition_value,
        }
    }
}
