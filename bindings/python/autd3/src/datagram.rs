use std::num::NonZeroU16;
use std::sync::Arc;

use autd3_python_capsule::{
    DevicePattern, capsule_of, modulation_from_capsule, pattern_from_capsule, to_pyerr,
};
use autd3_rs::value::{
    LoopBehavior as CoreLoopBehavior, ModulationBank as CoreModulationBank,
    PatternBank as CorePatternBank, PatternDataType as CorePatternDataType, SamplingConfig,
    TransitionMode as CoreTransitionMode,
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
    bank: CorePatternBank,
    emissions: Vec<DevicePattern>,
}

#[pymethods]
impl Pattern {
    #[new]
    #[pyo3(signature = (buffer, bank = None))]
    fn new(buffer: &Bound<'_, PyAny>, bank: Option<ops::PatternBank>) -> PyResult<Self> {
        let capsule = capsule_of(buffer)?;
        let emissions = pattern_from_capsule(&capsule)?;
        Ok(Self {
            bank: bank.map_or(CorePatternBank::B0, |b| b.0),
            emissions: emissions.to_vec(),
        })
    }
}

#[pyclass(name = "Modulation", module = "autd3")]
pub struct Modulation {
    bank: CoreModulationBank,
    divider: u16,
    data: Vec<u8>,
    loop_behavior: CoreLoopBehavior,
    transition_mode: CoreTransitionMode,
}

#[pymethods]
impl Modulation {
    #[new]
    #[pyo3(signature = (sampling_config, buffer, bank = None, loop_behavior = None, transition_mode = None))]
    fn new(
        sampling_config: &Bound<'_, PyAny>,
        buffer: &Bound<'_, PyAny>,
        bank: Option<ops::ModulationBank>,
        loop_behavior: Option<ops::LoopBehavior>,
        transition_mode: Option<ops::TransitionMode>,
    ) -> PyResult<Self> {
        let divider = sampling_config.call_method0("divide")?.extract::<u16>()?;
        let capsule = capsule_of(buffer)?;
        let data = modulation_from_capsule(&capsule)?.to_vec();
        Ok(Self {
            bank: bank.map_or(CoreModulationBank::B0, |b| b.0),
            divider,
            data,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
            transition_mode: transition_mode.map_or(CoreTransitionMode::Immediate, |t| t.0),
        })
    }
}

enum Pending {
    Pattern {
        bank: CorePatternBank,
        emissions: Vec<DevicePattern>,
    },
    Modulation {
        bank: CoreModulationBank,
        divider: u16,
        data: Vec<u8>,
        loop_behavior: CoreLoopBehavior,
        transition_mode: CoreTransitionMode,
    },
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
        loop_behavior: CoreLoopBehavior,
    },
    ChangePatternBank {
        bank: CorePatternBank,
        transition_mode: CoreTransitionMode,
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
        loop_behavior: CoreLoopBehavior,
    },
    ChangeModulationBank {
        bank: CoreModulationBank,
        transition_mode: CoreTransitionMode,
    },
    FociStm {
        config: autd3_rs::stm::StmConfig,
        points: crate::stm::FociPoints,
        option: autd3_rs::stm::FociStmOption,
    },
    PatternStm {
        config: autd3_rs::stm::StmConfig,
        patterns: Vec<Vec<DevicePattern>>,
        option: autd3_rs::stm::PatternStmOption,
    },
    Command(Box<dyn crate::commands::PushCommand>),
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
            let pattern = pattern.borrow();
            self.pending.push(Pending::Pattern {
                bank: pattern.bank,
                emissions: pattern.emissions.clone(),
            });
            return Ok(());
        }
        if let Ok(modulation) = obj.cast::<Modulation>() {
            let modulation = modulation.borrow();
            self.pending.push(Pending::Modulation {
                bank: modulation.bank,
                divider: modulation.divider,
                data: modulation.data.clone(),
                loop_behavior: modulation.loop_behavior,
                transition_mode: modulation.transition_mode,
            });
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
                loop_behavior: op.loop_behavior,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::ChangePatternBank>() {
            let op = op.borrow();
            self.pending.push(Pending::ChangePatternBank {
                bank: op.bank,
                transition_mode: op.transition_mode,
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
                loop_behavior: op.loop_behavior,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::ChangeModulationBank>() {
            let op = op.borrow();
            self.pending.push(Pending::ChangeModulationBank {
                bank: op.bank,
                transition_mode: op.transition_mode,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<crate::stm::FociStm>() {
            let op = op.borrow();
            self.pending.push(Pending::FociStm {
                config: op.config,
                points: op.points.clone(),
                option: op.option,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<crate::stm::PatternStm>() {
            let op = op.borrow();
            self.pending.push(Pending::PatternStm {
                config: op.config,
                patterns: op.patterns.clone(),
                option: op.option,
            });
            return Ok(());
        }
        if let Some(command) = crate::commands::boxed_command(obj) {
            self.pending.push(Pending::Command(command));
            return Ok(());
        }
        Err(PyValueError::new_err("Unknown datagram type"))
    }

    #[allow(clippy::too_many_lines)]
    fn build(&self, py: Python<'_>) -> PyResult<Datagrams> {
        let mut builder = CoreDatagramBuilder::new(self.num_devices);
        for pending in &self.pending {
            match pending {
                Pending::Pattern { bank, emissions } => {
                    builder.push(CorePattern::with_bank(*bank, emissions));
                }
                Pending::Modulation {
                    bank,
                    divider,
                    data,
                    loop_behavior,
                    transition_mode,
                } => {
                    let divider = NonZeroU16::new(*divider)
                        .ok_or_else(|| PyValueError::new_err("divider must be >= 1"))?;
                    let mut cmd =
                        CoreModulation::with_bank(*bank, SamplingConfig::Divide(divider), data);
                    cmd.loop_behavior = *loop_behavior;
                    cmd.transition_mode = *transition_mode;
                    builder.push(cmd);
                }
                Pending::WritePatternBuffer {
                    bank,
                    index,
                    emissions,
                } => {
                    builder.push(CoreWritePatternBuffer {
                        bank: *bank,
                        index: usize::from(*index),
                        emissions,
                    });
                }
                Pending::ConfigPattern {
                    bank,
                    divider,
                    size,
                    data_type,
                    loop_behavior,
                } => {
                    let divider = NonZeroU16::new(*divider)
                        .ok_or_else(|| PyValueError::new_err("divider must be >= 1"))?;
                    builder.push(CoreConfigPattern {
                        bank: *bank,
                        config: SamplingConfig::Divide(divider),
                        size: usize::try_from(*size).unwrap_or(usize::MAX),
                        data_type: *data_type,
                        loop_behavior: *loop_behavior,
                    });
                }
                Pending::ChangePatternBank {
                    bank,
                    transition_mode,
                } => {
                    builder.push(CoreChangePatternBank {
                        bank: *bank,
                        transition_mode: *transition_mode,
                    });
                }
                Pending::WriteModulationBuffer { bank, offset, data } => {
                    builder.push(CoreWriteModulationBuffer {
                        bank: *bank,
                        offset: usize::try_from(*offset).unwrap_or(usize::MAX),
                        data,
                    });
                }
                Pending::ConfigModulation {
                    bank,
                    divider,
                    size,
                    loop_behavior,
                } => {
                    let divider = NonZeroU16::new(*divider)
                        .ok_or_else(|| PyValueError::new_err("divider must be >= 1"))?;
                    builder.push(CoreConfigModulation {
                        bank: *bank,
                        config: SamplingConfig::Divide(divider),
                        size: usize::try_from(*size).unwrap_or(usize::MAX),
                        loop_behavior: *loop_behavior,
                    });
                }
                Pending::ChangeModulationBank {
                    bank,
                    transition_mode,
                } => {
                    builder.push(CoreChangeModulationBank {
                        bank: *bank,
                        transition_mode: *transition_mode,
                    });
                }
                Pending::FociStm {
                    config,
                    points,
                    option,
                } => {
                    points.push_into(*config, *option, &mut builder);
                }
                Pending::PatternStm {
                    config,
                    patterns,
                    option,
                } => {
                    builder.push(autd3_rs::stm::PatternStm::new(
                        *config,
                        patterns.as_slice(),
                        *option,
                    ));
                }
                Pending::Command(command) => {
                    command.push_into(&mut builder);
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
