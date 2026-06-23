use std::num::NonZeroU16;
use std::sync::Arc;

use autd3_python_capsule::{
    DevicePattern, capsule_of, modulation_from_capsule, pattern_from_capsule, to_pyerr,
};
use autd3_rs::value::{
    ModulationBank as CoreModulationBank, PatternBank as CorePatternBank,
    PatternDataType as CorePatternDataType, SamplingConfig, TransitionMode as CoreTransitionMode,
};
use autd3_rs::{
    ChangeModulationBank as CoreChangeModulationBank, ChangePatternBank as CoreChangePatternBank,
    ConfigModulation as CoreConfigModulation, ConfigPattern as CoreConfigPattern,
    DatagramBuilder as CoreDatagramBuilder, Datagrams as CoreDatagrams,
    Modulation as CoreModulation, Pattern as CorePattern,
    WriteModulationBuffer as CoreWriteModulationBuffer,
    WritePatternBuffer as CoreWritePatternBuffer,
};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;

use crate::ops;

#[pyclass(name = "Pattern", module = "autd3")]
pub struct Pattern {
    emissions: Vec<DevicePattern>,
}

#[pymethods]
impl Pattern {
    #[new]
    fn new(buffer: &Bound<'_, PyAny>) -> PyResult<Self> {
        let capsule = capsule_of(buffer)?;
        let emissions = pattern_from_capsule(&capsule)?;
        Ok(Self {
            emissions: emissions.to_vec(),
        })
    }
}

#[pyclass(name = "Modulation", module = "autd3")]
pub struct Modulation {
    divider: u16,
    data: Vec<u8>,
}

#[pymethods]
impl Modulation {
    #[new]
    fn new(sampling_config: &Bound<'_, PyAny>, buffer: &Bound<'_, PyAny>) -> PyResult<Self> {
        let divider = sampling_config.call_method0("divide")?.extract::<u16>()?;
        let capsule = capsule_of(buffer)?;
        let data = modulation_from_capsule(&capsule)?.to_vec();
        Ok(Self { divider, data })
    }
}

enum Pending {
    Pattern(Vec<DevicePattern>),
    Modulation(u16, Vec<u8>),
    WritePatternBuffer {
        bank: CorePatternBank,
        index: u16,
        emissions: Vec<DevicePattern>,
    },
    ConfigPattern {
        bank: CorePatternBank,
        divider: u16,
        size: u32,
        data_type: CorePatternDataType,
    },
    ChangePatternBank {
        bank: CorePatternBank,
        transition_mode: CoreTransitionMode,
        transition_value: u64,
    },
    WriteModulationBuffer {
        bank: CoreModulationBank,
        offset: u32,
        data: Vec<u8>,
    },
    ConfigModulation {
        bank: CoreModulationBank,
        divider: u16,
        size: u32,
    },
    ChangeModulationBank {
        bank: CoreModulationBank,
        transition_mode: CoreTransitionMode,
        transition_value: u64,
    },
}

#[pyclass(name = "DatagramBuilder", module = "autd3")]
pub struct DatagramBuilder {
    num_devices: usize,
    pending: Vec<Pending>,
}

impl DatagramBuilder {
    pub(crate) fn with_devices(num_devices: usize) -> Self {
        Self {
            num_devices,
            pending: Vec::new(),
        }
    }
}

#[pymethods]
impl DatagramBuilder {
    #[new]
    fn new(num_devices: usize) -> Self {
        Self::with_devices(num_devices)
    }

    fn push(&mut self, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(pattern) = obj.cast::<Pattern>() {
            self.pending
                .push(Pending::Pattern(pattern.borrow().emissions.clone()));
            return Ok(());
        }
        if let Ok(modulation) = obj.cast::<Modulation>() {
            let modulation = modulation.borrow();
            self.pending.push(Pending::Modulation(
                modulation.divider,
                modulation.data.clone(),
            ));
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::WritePatternBuffer>() {
            let op = op.borrow();
            self.pending.push(Pending::WritePatternBuffer {
                bank: op.bank,
                index: op.index,
                emissions: op.emissions.clone(),
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::ConfigPattern>() {
            let op = op.borrow();
            self.pending.push(Pending::ConfigPattern {
                bank: op.bank,
                divider: op.divider,
                size: op.size,
                data_type: op.data_type,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::ChangePatternBank>() {
            let op = op.borrow();
            self.pending.push(Pending::ChangePatternBank {
                bank: op.bank,
                transition_mode: op.transition_mode,
                transition_value: op.transition_value,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::WriteModulationBuffer>() {
            let op = op.borrow();
            self.pending.push(Pending::WriteModulationBuffer {
                bank: op.bank,
                offset: op.offset,
                data: op.data.clone(),
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::ConfigModulation>() {
            let op = op.borrow();
            self.pending.push(Pending::ConfigModulation {
                bank: op.bank,
                divider: op.divider,
                size: op.size,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::ChangeModulationBank>() {
            let op = op.borrow();
            self.pending.push(Pending::ChangeModulationBank {
                bank: op.bank,
                transition_mode: op.transition_mode,
                transition_value: op.transition_value,
            });
            return Ok(());
        }
        Err(PyValueError::new_err("Unknown datagram type"))
    }

    fn build(&self, py: Python<'_>) -> PyResult<Datagrams> {
        let mut builder = CoreDatagramBuilder::new(self.num_devices);
        for pending in &self.pending {
            match pending {
                Pending::Pattern(emissions) => {
                    builder.push(CorePattern::new(emissions));
                }
                Pending::Modulation(divider, data) => {
                    let divider = NonZeroU16::new(*divider)
                        .ok_or_else(|| PyValueError::new_err("divider must be >= 1"))?;
                    builder.push(CoreModulation::new(SamplingConfig::Divide(divider), data));
                }
                Pending::WritePatternBuffer {
                    bank,
                    index,
                    emissions,
                } => {
                    builder.push(CoreWritePatternBuffer {
                        bank: *bank,
                        index: *index,
                        emissions,
                    });
                }
                Pending::ConfigPattern {
                    bank,
                    divider,
                    size,
                    data_type,
                } => {
                    builder.push(CoreConfigPattern {
                        bank: *bank,
                        divider: *divider,
                        size: *size,
                        data_type: *data_type,
                    });
                }
                Pending::ChangePatternBank {
                    bank,
                    transition_mode,
                    transition_value,
                } => {
                    builder.push(CoreChangePatternBank {
                        bank: *bank,
                        transition_mode: *transition_mode,
                        transition_value: *transition_value,
                    });
                }
                Pending::WriteModulationBuffer { bank, offset, data } => {
                    builder.push(CoreWriteModulationBuffer {
                        bank: *bank,
                        offset: *offset,
                        data,
                    });
                }
                Pending::ConfigModulation {
                    bank,
                    divider,
                    size,
                } => {
                    builder.push(CoreConfigModulation {
                        bank: *bank,
                        divider: *divider,
                        size: *size,
                    });
                }
                Pending::ChangeModulationBank {
                    bank,
                    transition_mode,
                    transition_value,
                } => {
                    builder.push(CoreChangeModulationBank {
                        bank: *bank,
                        transition_mode: *transition_mode,
                        transition_value: *transition_value,
                    });
                }
            }
        }
        let datagrams = builder.build().map_err(|e| to_pyerr(py, e))?;
        Ok(Datagrams {
            inner: Arc::new(datagrams),
        })
    }
}

#[pyclass(name = "Frame", module = "autd3")]
pub struct Frame {
    pub(crate) datagrams: Arc<CoreDatagrams>,
    pub(crate) index: usize,
}

#[pyclass(name = "Datagrams", module = "autd3")]
pub struct Datagrams {
    pub(crate) inner: Arc<CoreDatagrams>,
}

#[pymethods]
impl Datagrams {
    fn num_frames(&self) -> usize {
        self.inner.len()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __getitem__(&self, index: usize) -> PyResult<Frame> {
        if index >= self.inner.len() {
            return Err(PyIndexError::new_err("frame index out of range"));
        }
        Ok(Frame {
            datagrams: Arc::clone(&self.inner),
            index,
        })
    }
}
