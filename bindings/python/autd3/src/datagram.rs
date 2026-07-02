use std::num::NonZeroU16;
use std::sync::Arc;

use autd3_python_capsule::{
    DevicePattern, capsule_of, modulation_from_capsule, pattern_from_capsule, to_pyerr,
};
use autd3_rs::commands::{
    ChangeModulationBank as CoreChangeModulationBank, ChangePatternBank as CoreChangePatternBank,
    Command as CoreCommand, ConfigFociStm as CoreConfigFociStm,
    ConfigModulation as CoreConfigModulation, ConfigPattern as CoreConfigPattern,
    Modulation as CoreModulation, Pattern as CorePattern,
    PatternCompression as CorePatternCompression,
    WriteModulationBuffer as CoreWriteModulationBuffer,
    WritePatternBuffer as CoreWritePatternBuffer,
    WritePatternCompressed as CoreWritePatternCompressed,
};
use autd3_rs::value::{
    LoopBehavior as CoreLoopBehavior, ModulationBank as CoreModulationBank,
    PatternBank as CorePatternBank, SamplingConfig, TransitionMode as CoreTransitionMode,
};
use autd3_rs::{DatagramBuilder as CoreDatagramBuilder, Frames as CoreFrames, Velocity};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;

use crate::ops;

#[pyclass(name = "Pattern", module = "autd3.commands")]
pub struct Pattern {
    bank: CorePatternBank,
    emissions: Vec<DevicePattern>,
}

#[pymethods]
impl Pattern {
    #[new]
    #[pyo3(signature = (emissions, bank = None))]
    fn new(emissions: &Bound<'_, PyAny>, bank: Option<ops::PatternBank>) -> PyResult<Self> {
        let capsule = capsule_of(emissions)?;
        let emissions = pattern_from_capsule(&capsule)?;
        Ok(Self {
            bank: bank.map_or(CorePatternBank::B0, |b| b.0),
            emissions: emissions.to_vec(),
        })
    }
}

#[pyclass(name = "Modulation", module = "autd3.commands")]
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
    #[pyo3(signature = (config, data, bank = None, loop_behavior = None, transition_mode = None))]
    fn new(
        config: &Bound<'_, PyAny>,
        data: &Bound<'_, PyAny>,
        bank: Option<ops::ModulationBank>,
        loop_behavior: Option<ops::LoopBehavior>,
        transition_mode: Option<ops::TransitionMode>,
    ) -> PyResult<Self> {
        let divider = config.call_method0("divide")?.extract::<u16>()?;
        let capsule = capsule_of(data)?;
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
        loop_behavior: CoreLoopBehavior,
    },
    ConfigFociStm {
        bank: CorePatternBank,
        divider: u16,
        size: u32,
        num_foci: u8,
        sound_speed: Velocity,
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
    WriteFociBuffer {
        bank: CorePatternBank,
        index_offset: usize,
        points: crate::stm::FociPoints,
    },
    WritePatternCompressed {
        bank: CorePatternBank,
        index: u32,
        format: CorePatternCompression,
        patterns: Vec<Vec<DevicePattern>>,
    },
    FociStm {
        config: autd3_rs::commands::StmConfig,
        points: crate::stm::FociPoints,
        option: autd3_rs::commands::FociStmOption,
    },
    PatternStm {
        config: autd3_rs::commands::StmConfig,
        patterns: Vec<Vec<DevicePattern>>,
        option: autd3_rs::commands::PatternStmOption,
    },
    Each {
        devices: Vec<Option<Pending>>,
    },
    Command(Box<dyn crate::commands::PushCommand>),
}

struct PendingCommand<'a>(&'a Pending);

impl<'a> CoreCommand<'a> for PendingCommand<'a> {
    fn expand(self, builder: &mut CoreDatagramBuilder<'a>) {
        push_pending(self.0, builder);
    }
}

fn validate_pending(pending: &Pending) -> PyResult<()> {
    match pending {
        Pending::Modulation { divider, .. }
        | Pending::ConfigPattern { divider, .. }
        | Pending::ConfigFociStm { divider, .. }
        | Pending::ConfigModulation { divider, .. } => NonZeroU16::new(*divider)
            .map(|_| ())
            .ok_or_else(|| PyValueError::new_err("divider must be >= 1")),
        Pending::Each { devices } => {
            for child in devices.iter().flatten() {
                validate_pending(child)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[allow(clippy::too_many_lines)]
fn push_pending<'a>(pending: &'a Pending, builder: &mut CoreDatagramBuilder<'a>) {
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
            let divider = NonZeroU16::new(*divider).unwrap_or(NonZeroU16::MIN);
            let mut cmd = CoreModulation::with_bank(*bank, SamplingConfig::new(divider), data);
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
            loop_behavior,
        } => {
            let divider = NonZeroU16::new(*divider).unwrap_or(NonZeroU16::MIN);
            builder.push(CoreConfigPattern {
                bank: *bank,
                config: SamplingConfig::new(divider),
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                loop_behavior: *loop_behavior,
            });
        }
        Pending::ConfigFociStm {
            bank,
            divider,
            size,
            num_foci,
            sound_speed,
            loop_behavior,
        } => {
            let divider = NonZeroU16::new(*divider).unwrap_or(NonZeroU16::MIN);
            builder.push(CoreConfigFociStm {
                bank: *bank,
                config: SamplingConfig::new(divider),
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                num_foci: *num_foci,
                sound_speed: *sound_speed,
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
            let divider = NonZeroU16::new(*divider).unwrap_or(NonZeroU16::MIN);
            builder.push(CoreConfigModulation {
                bank: *bank,
                config: SamplingConfig::new(divider),
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
        Pending::WriteFociBuffer {
            bank,
            index_offset,
            points,
        } => {
            points.push_write_foci(*bank, *index_offset, builder);
        }
        Pending::WritePatternCompressed {
            bank,
            index,
            format,
            patterns,
        } => {
            let mut arr: [Option<&[DevicePattern]>; 4] = [None, None, None, None];
            for (slot, p) in arr.iter_mut().zip(patterns.iter()) {
                *slot = Some(p.as_slice());
            }
            builder.push(CoreWritePatternCompressed {
                bank: *bank,
                index: usize::try_from(*index).unwrap_or(usize::MAX),
                format: *format,
                patterns: arr,
            });
        }
        Pending::FociStm {
            config,
            points,
            option,
        } => {
            points.push_into(*config, *option, builder);
        }
        Pending::PatternStm {
            config,
            patterns,
            option,
        } => {
            builder.push(autd3_rs::commands::PatternStm::new(
                *config,
                patterns.as_slice(),
                *option,
            ));
        }
        Pending::Each { devices } => {
            builder.push_each(|device| devices[device].as_ref().map(PendingCommand));
        }
        Pending::Command(command) => {
            command.push_into(builder);
        }
    }
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

    #[allow(clippy::too_many_lines)]
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
                loop_behavior: op.loop_behavior,
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::ConfigFociStm>() {
            let op = op.borrow();
            self.pending.push(Pending::ConfigFociStm {
                bank: op.bank,
                divider: op.divider,
                size: op.size,
                num_foci: op.num_foci,
                sound_speed: op.sound_speed,
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
        if let Ok(op) = obj.cast::<crate::stm::WriteFociBuffer>() {
            let op = op.borrow();
            self.pending.push(Pending::WriteFociBuffer {
                bank: op.bank,
                index_offset: op.index_offset,
                points: op.points.clone(),
            });
            return Ok(());
        }
        if let Ok(op) = obj.cast::<ops::WritePatternCompressed>() {
            let op = op.borrow();
            self.pending.push(Pending::WritePatternCompressed {
                bank: op.bank,
                index: op.index,
                format: op.format,
                patterns: op.patterns.clone(),
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

    fn push_each(&mut self, assign: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut devices = Vec::with_capacity(self.num_devices);
        for device in 0..self.num_devices {
            let result = assign.call1((device,))?;
            if result.is_none() {
                devices.push(None);
            } else {
                let mut tmp = DatagramBuilder::with_devices(self.num_devices);
                tmp.push(&result)?;
                devices.push(tmp.pending.pop());
            }
        }
        self.pending.push(Pending::Each { devices });
        Ok(())
    }

    fn build(&self, py: Python<'_>) -> PyResult<Frames> {
        let mut builder = CoreDatagramBuilder::new(self.num_devices);
        for pending in &self.pending {
            validate_pending(pending)?;
            push_pending(pending, &mut builder);
        }
        let frames = builder.build().map_err(|e| to_pyerr(py, e))?;
        Ok(Frames {
            inner: Arc::new(frames),
        })
    }
}

#[pyclass(name = "Frame", module = "autd3")]
pub struct Frame {
    pub(crate) datagrams: Arc<CoreFrames>,
    pub(crate) index: usize,
}

#[pyclass(name = "Frames", module = "autd3")]
pub struct Frames {
    pub(crate) inner: Arc<CoreFrames>,
}

#[pymethods]
impl Frames {
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
