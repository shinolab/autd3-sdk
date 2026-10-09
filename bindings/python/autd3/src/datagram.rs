use std::sync::Arc;

use autd3_python_capsule::extract::{extract_sampling_config, geometry_to_py};
use autd3_python_capsule::{
    capsule_of, client_pyerr, frame_into_capsule, geometry_from_capsule, intensities_from_capsule,
    modulation_from_capsule, phases_from_capsule,
};
use autd3_rs::commands::{
    Command as CoreCommand, Expansion, Modulation as CoreModulation, Pattern as CorePattern,
    PatternIntensity as CorePatternIntensity, each as core_each,
};
use autd3_rs::value::{
    Intensity, LoopBehavior as CoreLoopBehavior, ModulationBank as CoreModulationBank,
    PatternBank as CorePatternBank, Phase, SamplingConfig, TransitionMode as CoreTransitionMode,
};
use autd3_rs::{Frames as CoreFrames, Geometry};
use pyo3::exceptions::{PyIndexError, PyRecursionError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};

use crate::{commands, ops, stm};

pub(crate) trait PushCommand: Send + Sync {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error>;
}

impl<C> PushCommand for C
where
    C: for<'a> CoreCommand<'a> + Copy + Send + Sync,
{
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(*self)?;
        Ok(())
    }
}

pub(crate) enum OwnedPatternIntensity {
    Uniform(Intensity),
    PerDevice(Vec<Vec<Intensity>>),
}

impl OwnedPatternIntensity {
    pub(crate) fn as_ref(&self) -> CorePatternIntensity<'_> {
        match self {
            OwnedPatternIntensity::Uniform(intensity) => CorePatternIntensity::Uniform(*intensity),
            OwnedPatternIntensity::PerDevice(intensities) => {
                CorePatternIntensity::PerDevice(intensities)
            }
        }
    }
}

pub(crate) fn extract_phases(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<Phase>>> {
    let capsule = capsule_of(obj)?;
    Ok(phases_from_capsule(&capsule)?.to_vec())
}

pub(crate) fn extract_intensities(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<Intensity>>> {
    let capsule = capsule_of(obj)?;
    Ok(intensities_from_capsule(&capsule)?.to_vec())
}

pub(crate) fn extract_uniform_intensity(obj: &Bound<'_, PyAny>) -> Option<Intensity> {
    obj.extract::<u8>().ok().map(Intensity)
}

pub(crate) fn extract_pattern_intensity(obj: &Bound<'_, PyAny>) -> PyResult<OwnedPatternIntensity> {
    if let Some(intensity) = extract_uniform_intensity(obj) {
        return Ok(OwnedPatternIntensity::Uniform(intensity));
    }
    Ok(OwnedPatternIntensity::PerDevice(extract_intensities(obj)?))
}

pub(crate) struct PatternData {
    bank: CorePatternBank,
    phases: Vec<Vec<Phase>>,
    intensities: OwnedPatternIntensity,
    transition_mode: CoreTransitionMode,
}

impl PushCommand for PatternData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(CorePattern {
            transition_mode: self.transition_mode,
            ..CorePattern::with_bank(self.bank, &self.phases, self.intensities.as_ref())
        })?;
        Ok(())
    }
}

#[pyclass(name = "Pattern", module = "autd3.commands", frozen)]
pub struct Pattern(pub(crate) Arc<PatternData>);

#[pymethods]
impl Pattern {
    #[new]
    #[pyo3(signature = (phases, intensities, bank = None, transition_mode = None))]
    fn new(
        phases: &Bound<'_, PyAny>,
        intensities: &Bound<'_, PyAny>,
        bank: Option<ops::PatternBank>,
        transition_mode: Option<ops::TransitionMode>,
    ) -> PyResult<Self> {
        Ok(Self(Arc::new(PatternData {
            bank: bank.map_or(CorePatternBank::B0, |b| b.0),
            phases: extract_phases(phases)?,
            intensities: extract_pattern_intensity(intensities)?,
            transition_mode: transition_mode.map_or(CoreTransitionMode::Immediate, |t| t.0),
        })))
    }
}

pub(crate) struct ModulationData {
    bank: CoreModulationBank,
    config: SamplingConfig,
    data: Vec<u8>,
    loop_behavior: CoreLoopBehavior,
    transition_mode: CoreTransitionMode,
}

impl PushCommand for ModulationData {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        let mut cmd = CoreModulation::with_bank(self.bank, self.config, &self.data);
        cmd.loop_behavior = self.loop_behavior;
        cmd.transition_mode = self.transition_mode;
        expansion.push(cmd)?;
        Ok(())
    }
}

#[pyclass(name = "Modulation", module = "autd3.commands", frozen)]
pub struct Modulation(pub(crate) Arc<ModulationData>);

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
        let config = extract_sampling_config(config)?;
        let capsule = capsule_of(data)?;
        let data = modulation_from_capsule(&capsule)?.to_vec();
        Ok(Self(Arc::new(ModulationData {
            bank: bank.map_or(CoreModulationBank::B0, |b| b.0),
            config,
            data,
            loop_behavior: loop_behavior.map_or(CoreLoopBehavior::Infinite, |l| l.0),
            transition_mode: transition_mode.map_or(CoreTransitionMode::Immediate, |t| t.0),
        })))
    }
}

struct PerDevice(Vec<Option<Arc<dyn PushCommand>>>);

struct Sequence(Vec<Arc<dyn PushCommand>>);

pub(crate) struct Expand<'a>(pub(crate) &'a dyn PushCommand);

impl<'a> CoreCommand<'a> for Expand<'a> {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        self.0.push_into(expansion)
    }
}

impl PushCommand for PerDevice {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        expansion.push(core_each(|device| {
            self.0[device.idx()].as_deref().map(Expand)
        }))?;
        Ok(())
    }
}

impl PushCommand for Sequence {
    fn push_into<'a>(&'a self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        for command in &self.0 {
            command.push_into(expansion)?;
        }
        Ok(())
    }
}

#[pyclass(name = "Each", module = "autd3.commands", frozen)]
pub struct Each {
    assign: Py<PyAny>,
}

#[pyfunction]
pub(crate) fn each(assign: Bound<'_, PyAny>) -> PyResult<Each> {
    if !assign.is_callable() {
        return Err(PyTypeError::new_err(
            "each expects a callable taking a device",
        ));
    }
    Ok(Each {
        assign: assign.unbind(),
    })
}

const COMMAND_NESTING_MAX: usize = 64;

fn sequence_of<'py>(
    items: impl Iterator<Item = Bound<'py, PyAny>>,
    geometry: &Geometry,
    depth: usize,
) -> PyResult<Arc<dyn PushCommand>> {
    let commands = items
        .map(|item| nested_command_of(&item, geometry, depth + 1))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Arc::new(Sequence(commands)))
}

pub(crate) fn command_of(
    obj: &Bound<'_, PyAny>,
    geometry: &Geometry,
) -> PyResult<Arc<dyn PushCommand>> {
    nested_command_of(obj, geometry, 0)
}

fn nested_command_of(
    obj: &Bound<'_, PyAny>,
    geometry: &Geometry,
    depth: usize,
) -> PyResult<Arc<dyn PushCommand>> {
    if depth > COMMAND_NESTING_MAX {
        return Err(PyRecursionError::new_err(format!(
            "commands are nested deeper than {COMMAND_NESTING_MAX} levels"
        )));
    }
    macro_rules! try_cast {
        ($($ty:ty),* $(,)?) => {
            $(if let Ok(c) = obj.cast::<$ty>() {
                let command: Arc<dyn PushCommand> = c.get().0.clone();
                return Ok(command);
            })*
        };
    }
    try_cast!(
        Pattern,
        Modulation,
        ops::WritePatternBuffer,
        ops::WritePatternPhase,
        ops::ConfigPattern,
        ops::ConfigFociStm,
        ops::ActivatePatternBank,
        ops::WriteModulationBuffer,
        ops::ConfigModulation,
        ops::ActivateModulationBank,
        stm::WriteFociBuffer,
        stm::FociStm,
        stm::PatternStm,
        commands::Clear,
        commands::Synchronize,
        commands::ReleaseFailsafe,
        commands::Nop,
        commands::ForceFan,
        commands::SetSilencerPy,
        commands::SetCpuConfigPy,
        commands::SetGpioOutPy,
        commands::EmulateGpioInPy,
        commands::SetOutputMaskPy,
        commands::SetPhaseCorrectionPy,
        commands::SetPulseWidthTablePy,
    );
    if let Ok(per_device) = obj.cast::<Each>() {
        let py = obj.py();
        let assign = per_device.get().assign.bind(py);
        let devices = geometry_to_py(py, geometry)?;
        let commands = (0..geometry.num_devices())
            .map(|device| {
                let assigned = assign.call1((devices.get_item(device)?,))?;
                if assigned.is_none() {
                    Ok(None)
                } else {
                    nested_command_of(&assigned, geometry, depth + 1).map(Some)
                }
            })
            .collect::<PyResult<Vec<_>>>()?;
        return Ok(Arc::new(PerDevice(commands)));
    }
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        return sequence_of(tuple.iter(), geometry, depth);
    }
    if let Ok(list) = obj.cast::<PyList>() {
        return sequence_of(list.iter(), geometry, depth);
    }
    Err(PyTypeError::new_err(format!(
        "expected a command, each(...), or a tuple/list of them, got {}",
        obj.get_type().name()?
    )))
}

#[pyclass(name = "Frame", module = "autd3")]
pub struct Frame {
    pub(crate) datagrams: Arc<CoreFrames>,
    pub(crate) index: usize,
}

#[pymethods]
impl Frame {
    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyCapsule>> {
        frame_into_capsule(py, Arc::clone(&self.datagrams), self.index)
    }
}

#[pyclass(name = "Frames", module = "autd3")]
pub struct Frames(pub(crate) Arc<CoreFrames>);

#[pymethods]
impl Frames {
    #[new]
    fn new() -> Self {
        Self(Arc::new(CoreFrames::default()))
    }

    #[staticmethod]
    fn encode(
        py: Python<'_>,
        geometry: &Bound<'_, PyAny>,
        command: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let capsule = capsule_of(geometry)?;
        let geometry = geometry_from_capsule(&capsule)?;
        let command = command_of(command, geometry)?;
        let frames = CoreFrames::encode(geometry, Expand(command.as_ref()))
            .map_err(|e| client_pyerr(py, &e))?;
        Ok(Self(Arc::new(frames)))
    }

    fn encode_into(
        slf: &Bound<'_, Self>,
        geometry: &Bound<'_, PyAny>,
        command: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = slf.py();
        let capsule = capsule_of(geometry)?;
        let geometry = geometry_from_capsule(&capsule)?;
        let command = command_of(command, geometry)?;
        let mut this = slf.try_borrow_mut()?;
        if Arc::get_mut(&mut this.0).is_none() {
            this.0 = Arc::new(CoreFrames::default());
        }
        let frames = Arc::get_mut(&mut this.0).expect("the frames were just made unique");
        frames
            .encode_into(geometry, Expand(command.as_ref()))
            .map_err(|e| client_pyerr(py, &e))
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn __len__(&self) -> usize {
        self.0.len()
    }

    fn __getitem__(&self, index: usize) -> PyResult<Frame> {
        if index >= self.0.len() {
            return Err(PyIndexError::new_err("frame index out of range"));
        }
        Ok(Frame {
            datagrams: Arc::clone(&self.0),
            index,
        })
    }
}
