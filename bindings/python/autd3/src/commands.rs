use core::num::NonZeroU16;
use core::time::Duration;

use autd3_rs::DatagramBuilder as CoreDatagramBuilder;
use autd3_rs::commands::{
    Clear as CoreClear, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate,
    ForceFan as CoreForceFan, GpioOut as CoreGpioOut, Nop as CoreNop, PWE_TABLE_SIZE, SetGpioOut,
    SetOutputMask, SetPhaseCorrection, SetPulseWidthTable as CoreSetPulseWidthTable, SetSilencer,
    Synchronize as CoreSynchronize,
};
use autd3_rs::geometry::Autd3;
use autd3_rs::value::{Phase, PulseWidth as CorePulseWidth};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

fn extract_u8(obj: &Bound<'_, PyAny>) -> PyResult<u8> {
    if let Ok(v) = obj.extract::<u8>() {
        return Ok(v);
    }
    obj.getattr("value")?.extract::<u8>()
}

fn extract_duration(obj: &Bound<'_, PyAny>) -> PyResult<Duration> {
    let nanos = obj.call_method0("as_nanos")?.extract::<u128>()?;
    u64::try_from(nanos)
        .map(Duration::from_nanos)
        .map_err(|_| PyValueError::new_err("duration is out of range"))
}

pub(crate) trait PushCommand: Send + Sync {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>);
}

macro_rules! simple_command {
    ($pyname:literal, $py:ident, $data:ident, |$self:ident, $builder:ident| $body:block, $new:item) => {
        #[derive(Clone)]
        pub(crate) struct $data;

        impl PushCommand for $data {
            fn push_into<'a>(&'a $self, $builder: &mut CoreDatagramBuilder<'a>) $body
        }

        #[pyclass(name = $pyname, module = "autd3.commands")]
        pub struct $py;

        #[pymethods]
        impl $py {
            $new
        }

        impl $py {
            pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
                Box::new($data)
            }
        }
    };
}

simple_command!(
    "Clear",
    Clear,
    ClearCmd,
    |self, builder| {
        builder.push(CoreClear);
    },
    #[new]
    fn new() -> Self {
        Self
    }
);

simple_command!(
    "Synchronize",
    Synchronize,
    SynchronizeCmd,
    |self, builder| {
        builder.push(CoreSynchronize);
    },
    #[new]
    fn new() -> Self {
        Self
    }
);

simple_command!(
    "Nop",
    Nop,
    NopCmd,
    |self, builder| {
        builder.push(CoreNop);
    },
    #[new]
    fn new() -> Self {
        Self
    }
);

#[derive(Clone)]
struct ForceFanCmd {
    value: bool,
}

impl PushCommand for ForceFanCmd {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>) {
        builder.push(CoreForceFan { value: self.value });
    }
}

#[pyclass(name = "ForceFan", module = "autd3.commands")]
pub struct ForceFan {
    value: bool,
}

#[pymethods]
impl ForceFan {
    #[new]
    fn new(value: bool) -> Self {
        Self { value }
    }
}

impl ForceFan {
    pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
        Box::new(ForceFanCmd { value: self.value })
    }
}

#[derive(Clone, Copy)]
enum SilencerConfigKind {
    Completion(FixedCompletionTime),
    UpdateRate(FixedUpdateRate),
}

#[pyclass(
    name = "FixedCompletionTime",
    module = "autd3.commands",
    skip_from_py_object
)]
pub struct FixedCompletionTimePy {
    inner: FixedCompletionTime,
}

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
        Ok(Self {
            inner: FixedCompletionTime {
                intensity: intensity
                    .map(extract_duration)
                    .transpose()?
                    .unwrap_or(default.intensity),
                phase: phase
                    .map(extract_duration)
                    .transpose()?
                    .unwrap_or(default.phase),
                strict_mode,
            },
        })
    }
}

#[pyclass(
    name = "FixedUpdateRate",
    module = "autd3.commands",
    skip_from_py_object
)]
pub struct FixedUpdateRatePy {
    inner: FixedUpdateRate,
}

#[pymethods]
impl FixedUpdateRatePy {
    #[new]
    #[pyo3(signature = (intensity = 256, phase = 256))]
    fn new(intensity: u16, phase: u16) -> PyResult<Self> {
        Ok(Self {
            inner: FixedUpdateRate {
                intensity: NonZeroU16::new(intensity)
                    .ok_or_else(|| PyValueError::new_err("intensity must be >= 1"))?,
                phase: NonZeroU16::new(phase)
                    .ok_or_else(|| PyValueError::new_err("phase must be >= 1"))?,
            },
        })
    }
}

struct SetSilencerCmd {
    config: SilencerConfigKind,
}

impl PushCommand for SetSilencerCmd {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>) {
        match self.config {
            SilencerConfigKind::Completion(c) => {
                builder.push(SetSilencer::new(c));
            }
            SilencerConfigKind::UpdateRate(c) => {
                builder.push(SetSilencer::new(c));
            }
        }
    }
}

#[pyclass(name = "SetSilencer", module = "autd3.commands")]
pub struct SetSilencerPy {
    config: SilencerConfigKind,
}

#[pymethods]
impl SetSilencerPy {
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let Some(config) = config else {
            return Ok(Self {
                config: SilencerConfigKind::Completion(SetSilencer::default().config),
            });
        };
        let config = if let Ok(c) = config.cast::<FixedCompletionTimePy>() {
            SilencerConfigKind::Completion(c.borrow().inner)
        } else if let Ok(c) = config.cast::<FixedUpdateRatePy>() {
            SilencerConfigKind::UpdateRate(c.borrow().inner)
        } else {
            return Err(PyValueError::new_err(
                "SetSilencer expects a FixedCompletionTime or FixedUpdateRate",
            ));
        };
        Ok(Self { config })
    }

    #[staticmethod]
    fn disable() -> Self {
        Self {
            config: SilencerConfigKind::Completion(SetSilencer::disable().config),
        }
    }
}

impl SetSilencerPy {
    pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
        Box::new(SetSilencerCmd {
            config: self.config,
        })
    }
}

#[pyclass(name = "GpioOut", module = "autd3.commands", from_py_object)]
#[derive(Clone, Copy)]
pub struct GpioOut(pub(crate) CoreGpioOut);

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
    fn sys_time_eq(sys_time: u64) -> Self {
        Self(CoreGpioOut::SysTimeEq(sys_time))
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
}

#[derive(Clone)]
struct SetGpioOutCmd {
    outputs: [CoreGpioOut; 4],
}

impl PushCommand for SetGpioOutCmd {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>) {
        builder.push(SetGpioOut {
            outputs: self.outputs,
        });
    }
}

#[pyclass(name = "SetGpioOut", module = "autd3.commands")]
pub struct SetGpioOutPy {
    outputs: [CoreGpioOut; 4],
}

#[pymethods]
impl SetGpioOutPy {
    #[new]
    fn new(outputs: Vec<GpioOut>) -> PyResult<Self> {
        let outputs: [GpioOut; 4] = outputs
            .try_into()
            .map_err(|_| PyValueError::new_err("SetGpioOut needs exactly 4 outputs"))?;
        Ok(Self {
            outputs: outputs.map(|g| g.0),
        })
    }
}

impl SetGpioOutPy {
    pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
        Box::new(SetGpioOutCmd {
            outputs: self.outputs,
        })
    }
}

#[derive(Clone)]
struct EmulateGpioInCmd {
    values: [bool; 4],
}

impl PushCommand for EmulateGpioInCmd {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>) {
        builder.push(EmulateGpioIn {
            values: self.values,
        });
    }
}

#[pyclass(name = "EmulateGpioIn", module = "autd3.commands")]
pub struct EmulateGpioInPy {
    values: [bool; 4],
}

#[pymethods]
impl EmulateGpioInPy {
    #[new]
    fn new(values: Vec<bool>) -> PyResult<Self> {
        let values: [bool; 4] = values
            .try_into()
            .map_err(|_| PyValueError::new_err("EmulateGpioIn needs exactly 4 values"))?;
        Ok(Self { values })
    }
}

impl EmulateGpioInPy {
    pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
        Box::new(EmulateGpioInCmd {
            values: self.values,
        })
    }
}

#[derive(Clone)]
struct SetOutputMaskCmd {
    masks: Vec<Vec<bool>>,
}

impl PushCommand for SetOutputMaskCmd {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>) {
        builder.push(SetOutputMask {
            masks: self.masks.as_slice(),
        });
    }
}

#[pyclass(name = "SetOutputMask", module = "autd3.commands")]
pub struct SetOutputMaskPy {
    masks: Vec<Vec<bool>>,
}

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
        Ok(Self { masks })
    }
}

impl SetOutputMaskPy {
    pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
        Box::new(SetOutputMaskCmd {
            masks: self.masks.clone(),
        })
    }
}

#[derive(Clone)]
struct SetPhaseCorrectionCmd {
    phases: Vec<Vec<Phase>>,
}

impl PushCommand for SetPhaseCorrectionCmd {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>) {
        builder.push(SetPhaseCorrection {
            phases: self.phases.as_slice(),
        });
    }
}

#[pyclass(name = "SetPhaseCorrection", module = "autd3.commands")]
pub struct SetPhaseCorrectionPy {
    phases: Vec<Vec<Phase>>,
}

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
        Ok(Self { phases })
    }
}

impl SetPhaseCorrectionPy {
    pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
        Box::new(SetPhaseCorrectionCmd {
            phases: self.phases.clone(),
        })
    }
}

#[derive(Clone)]
struct SetPulseWidthTableCmd {
    table: [CorePulseWidth; PWE_TABLE_SIZE],
}

impl PushCommand for SetPulseWidthTableCmd {
    fn push_into<'a>(&'a self, builder: &mut CoreDatagramBuilder<'a>) {
        builder.push(CoreSetPulseWidthTable { table: &self.table });
    }
}

#[pyclass(name = "SetPulseWidthTable", module = "autd3.commands")]
pub struct SetPulseWidthTablePy {
    table: [u16; PWE_TABLE_SIZE],
}

#[pymethods]
impl SetPulseWidthTablePy {
    #[new]
    fn new(table: Vec<u16>) -> PyResult<Self> {
        let table: [u16; PWE_TABLE_SIZE] = table.try_into().map_err(|v: Vec<u16>| {
            PyValueError::new_err(format!(
                "SetPulseWidthTable needs exactly {PWE_TABLE_SIZE} entries, got {}",
                v.len()
            ))
        })?;
        Ok(Self { table })
    }

    #[staticmethod]
    fn default_table() -> Vec<u16> {
        CoreSetPulseWidthTable::default_table()
            .into_iter()
            .map(|pw| pw.pulse_width().unwrap_or(0))
            .collect()
    }
}

impl SetPulseWidthTablePy {
    pub(crate) fn boxed(&self) -> Box<dyn PushCommand> {
        Box::new(SetPulseWidthTableCmd {
            table: self.table.map(CorePulseWidth::new),
        })
    }
}

#[pyclass(name = "PulseWidth", module = "autd3.value")]
pub struct PulseWidth;

#[pymethods]
impl PulseWidth {
    #[staticmethod]
    fn from_duty(duty: f32) -> PyResult<u16> {
        CorePulseWidth::from_duty(duty)
            .pulse_width()
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_raw(pulse_width: u16) -> PyResult<u16> {
        CorePulseWidth::new(pulse_width)
            .pulse_width()
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn default_table() -> Vec<u16> {
        CoreSetPulseWidthTable::default_table()
            .into_iter()
            .map(|pw| pw.pulse_width().unwrap_or(0))
            .collect()
    }
}

pub(crate) fn boxed_command(obj: &Bound<'_, PyAny>) -> Option<Box<dyn PushCommand>> {
    if let Ok(c) = obj.cast::<Clear>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<Synchronize>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<Nop>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<ForceFan>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<SetSilencerPy>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<SetGpioOutPy>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<EmulateGpioInPy>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<SetOutputMaskPy>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<SetPhaseCorrectionPy>() {
        return Some(c.borrow().boxed());
    }
    if let Ok(c) = obj.cast::<SetPulseWidthTablePy>() {
        return Some(c.borrow().boxed());
    }
    None
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Clear>()?;
    m.add_class::<Synchronize>()?;
    m.add_class::<Nop>()?;
    m.add_class::<ForceFan>()?;
    m.add_class::<FixedCompletionTimePy>()?;
    m.add_class::<FixedUpdateRatePy>()?;
    m.add_class::<SetSilencerPy>()?;
    m.add_class::<GpioOut>()?;
    m.add_class::<SetGpioOutPy>()?;
    m.add_class::<EmulateGpioInPy>()?;
    m.add_class::<SetOutputMaskPy>()?;
    m.add_class::<SetPhaseCorrectionPy>()?;
    m.add_class::<SetPulseWidthTablePy>()?;
    m.add_class::<PulseWidth>()?;
    Ok(())
}
