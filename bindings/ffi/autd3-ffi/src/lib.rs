use std::ffi::{c_char, c_void};
use std::future::Future;
use std::num::NonZeroU16;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

mod logging;
mod udp;

use autd3_binding_rt::Executor;
use autd3_ffi_abi::{
    AUTD3_ERR, AUTD3_ERR_DEVICE, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_ERR_NETWORK, AUTD3_ERR_TIMEOUT,
    AUTD3_OK, Autd3FirmwareVersion, Autd3Telemetry, Buffer, CompletionCallback, CompletionCtx,
    IntensityBuffer, ModulationBuffer, PhaseBuffer, drop_handle, handle_mut, handle_ref,
    into_handle, slice_mut, slice_ref, take_handle, to_ns, write_cstr, write_out,
};
use autd3_rs::commands::{
    ActivateModulationBank, ActivatePatternBank, BoxedCommand, Clear, Command, ConfigFociStm,
    ConfigModulation, ConfigPattern, CpuConfig, EmulateGpioIn, FixedCompletionTime,
    FixedUpdateRate, FociStm as CoreFociStm, FociStmOption, ForceFan, FpgaBusWait, FpgaConfig,
    GpioOut, Modulation, Nop, Operation, PWE_TABLE_SIZE, Pattern, PatternIntensity, PatternStm,
    PatternStmOption, PhaseDepth, PtpConfig, ReleaseFailsafe, SetCpuConfig, SetFpgaConfig,
    SetGpioOut, SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, StmIntensity,
    Synchronize, WriteFociBuffer, WriteModulationBuffer, WritePatternBuffer, WritePatternPhase,
    each,
};
use autd3_rs::params::NUM_FOCI_MAX;
use autd3_rs::value::{
    ControlPoint, ControlPoints, Intensity, LoopBehavior, ModulationBank, PatternBank, Phase,
    PulseWidth, SamplingConfig, SysTime, TransitionMode,
};
use autd3_rs::{
    Client, DeviceState, Error, ExecAt, FirmwareVersion, FpgaState, Frames, Geometry, Point3,
    Response, ResponseFuture, Telemetry, Velocity,
};

fn to_pattern_bank(v: u8) -> Option<PatternBank> {
    match v {
        0 => Some(PatternBank::B0),
        1 => Some(PatternBank::B1),
        _ => None,
    }
}

fn to_modulation_bank(v: u8) -> Option<ModulationBank> {
    match v {
        0 => Some(ModulationBank::B0),
        1 => Some(ModulationBank::B1),
        _ => None,
    }
}

fn to_telemetry(telemetry: Telemetry) -> Autd3Telemetry {
    Autd3Telemetry {
        fifo_drop: telemetry.fifo_drop,
        dispatch_error: telemetry.dispatch_error,
        processed: telemetry.processed,
        send_failure: telemetry.send_failure,
        failsafe: telemetry.failsafe,
        ptp_unlock_failsafe: telemetry.ptp_unlock_failsafe,
        sync_resync: telemetry.sync_resync,
        boot_failure: telemetry.boot_failure,
        cpu_watchdog: telemetry.cpu_watchdog,
        exec_late: telemetry.exec_late,
    }
}

#[repr(C)]
pub struct Autd3GpioOut {
    pub kind: u8,
    pub value: u64,
}

#[allow(clippy::cast_possible_truncation)]
fn to_gpio_out(g: &Autd3GpioOut) -> Option<GpioOut> {
    match g.kind {
        0 => Some(GpioOut::Off),
        1 => Some(GpioOut::BaseSignal),
        2 => Some(GpioOut::Thermo),
        3 => Some(GpioOut::ForceFan),
        4 => Some(GpioOut::Sync),
        5 => Some(GpioOut::ModBank),
        6 => Some(GpioOut::ModIdx(g.value as u16)),
        7 => Some(GpioOut::PatternBank),
        8 => Some(GpioOut::PatternIdx(g.value as u16)),
        9 => Some(GpioOut::IsStmMode),
        10 => Some(GpioOut::SysTimeEq(SysTime::from_nanos(g.value))),
        11 => Some(GpioOut::SyncDiff),
        12 => Some(GpioOut::PwmOut(g.value as u8)),
        13 => Some(GpioOut::Direct(g.value != 0)),
        _ => None,
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Autd3ExecAt {
    pub kind: u8,
    pub ns: u64,
}

fn to_exec_at(e: Autd3ExecAt) -> Option<ExecAt> {
    match e.kind {
        0 => Some(ExecAt::Immediate),
        1 => Some(ExecAt::Lead(Duration::from_nanos(e.ns))),
        2 => Some(ExecAt::At(SysTime::from_nanos(e.ns))),
        _ => None,
    }
}

fn rep_to_loop_behavior(rep: u16) -> LoopBehavior {
    if rep == 0xFFFF {
        LoopBehavior::Infinite
    } else {
        NonZeroU16::new(rep + 1).map_or(LoopBehavior::Infinite, LoopBehavior::Finite)
    }
}

#[repr(C)]
pub struct Autd3StmControlPoint {
    pub point: [f32; 3],
    pub phase_offset: u8,
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_write_foci_buffer_max_len(num_foci: u8) -> usize {
    WriteFociBuffer::max_len(num_foci)
}

unsafe fn foci_points(
    points: *const Autd3StmControlPoint,
    num_samples: usize,
    num_foci: u8,
    intensities: *const u8,
) -> Option<Vec<ControlPoints>> {
    if !(1..=NUM_FOCI_MAX).contains(&num_foci) {
        return None;
    }
    let n = usize::from(num_foci);
    let points = unsafe { slice_ref(points, num_samples.checked_mul(n)?) }?;
    let intensities = unsafe { slice_ref(intensities, num_samples) }?;
    Some(
        points
            .chunks_exact(n)
            .zip(intensities)
            .map(|(chunk, &intensity)| {
                ControlPoints::new(
                    chunk
                        .iter()
                        .map(|p| {
                            ControlPoint::new(
                                Point3::new(p.point[0], p.point[1], p.point[2]),
                                Phase(p.phase_offset),
                            )
                        })
                        .collect::<Vec<_>>(),
                    Intensity(intensity),
                )
            })
            .collect(),
    )
}

unsafe fn clone_buffers<T: Clone>(
    buffers: *const *const Buffer<T>,
    len: usize,
) -> Option<Vec<Vec<Vec<T>>>> {
    unsafe { slice_ref(buffers, len) }?
        .iter()
        .map(|&p| unsafe { handle_ref(p) }.map(|buffer| buffer.0.clone()))
        .collect()
}

fn to_sampling_config(divide: u16) -> Option<SamplingConfig> {
    NonZeroU16::new(divide).map(SamplingConfig::new)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_params_pwe_table_size() -> usize {
    PWE_TABLE_SIZE
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_foci_stm_option_default_sound_speed() -> f32 {
    FociStmOption::default().sound_speed.m_s()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_silencer_default_completion_time(
    out_intensity_ns: *mut u64,
    out_phase_ns: *mut u64,
    out_strict_mode: *mut bool,
) -> i32 {
    let default = FixedCompletionTime::default();
    let codes = [
        unsafe { write_out(out_intensity_ns, to_ns(default.intensity)) },
        unsafe { write_out(out_phase_ns, to_ns(default.phase)) },
        unsafe { write_out(out_strict_mode, default.strict_mode) },
    ];
    codes
        .into_iter()
        .find(|&code| code != AUTD3_OK)
        .unwrap_or(AUTD3_OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_is_thermal_asserted(raw: u8) -> bool {
    FpgaState(raw).is_thermal_asserted()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_current_mod_bank(raw: u8) -> u8 {
    match FpgaState(raw).current_mod_bank() {
        ModulationBank::B0 => 0,
        ModulationBank::B1 => 1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_current_pattern_bank(raw: u8) -> u8 {
    match FpgaState(raw).current_pattern_bank() {
        PatternBank::B0 => 0,
        PatternBank::B1 => 1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_is_pattern_mode(raw: u8) -> bool {
    FpgaState(raw).is_pattern_mode()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_is_pattern_stopped(raw: u8) -> bool {
    FpgaState(raw).is_pattern_stopped()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_is_mod_stopped(raw: u8) -> bool {
    FpgaState(raw).is_mod_stopped()
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_is_transition_pending(raw: u8) -> bool {
    FpgaState(raw).is_transition_pending()
}

pub enum OwnedPatternIntensity {
    Uniform(Intensity),
    PerDevice(Vec<Vec<Intensity>>),
}

impl OwnedPatternIntensity {
    fn as_ref(&self) -> PatternIntensity<'_> {
        match self {
            OwnedPatternIntensity::Uniform(intensity) => PatternIntensity::Uniform(*intensity),
            OwnedPatternIntensity::PerDevice(intensities) => {
                PatternIntensity::PerDevice(intensities)
            }
        }
    }
}

pub enum OwnedStmIntensity {
    Uniform(Intensity),
    SameForAllIndices(Vec<Vec<Intensity>>),
    PerIndex(Vec<Vec<Vec<Intensity>>>),
}

impl OwnedStmIntensity {
    fn as_ref(&self) -> StmIntensity<'_> {
        match self {
            OwnedStmIntensity::Uniform(intensity) => StmIntensity::Uniform(*intensity),
            OwnedStmIntensity::SameForAllIndices(intensities) => {
                StmIntensity::SameForAllIndices(intensities)
            }
            OwnedStmIntensity::PerIndex(intensities) => StmIntensity::PerIndex(intensities),
        }
    }
}

pub enum OwnedCommand {
    Each(Vec<Option<OwnedCommand>>),
    Pattern {
        phases: Vec<Vec<Phase>>,
        intensities: OwnedPatternIntensity,
        bank: PatternBank,
        transition_mode: TransitionMode,
    },
    Modulation {
        config: SamplingConfig,
        data: Vec<u8>,
        bank: ModulationBank,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    },
    WritePatternBuffer {
        bank: PatternBank,
        start_index: u16,
        patterns: Vec<Vec<Vec<Phase>>>,
        intensities: OwnedStmIntensity,
    },
    WriteFociBuffer {
        bank: PatternBank,
        start_index: usize,
        points: Vec<ControlPoints>,
    },
    WritePatternPhase {
        bank: PatternBank,
        start_index: u16,
        depth: PhaseDepth,
        intensity: Intensity,
        patterns: Vec<Vec<Vec<Phase>>>,
    },
    ConfigPattern {
        bank: PatternBank,
        config: SamplingConfig,
        size: u32,
        loop_behavior: LoopBehavior,
    },
    ConfigFociStm {
        bank: PatternBank,
        config: SamplingConfig,
        size: u32,
        num_foci: u8,
        sound_speed: Velocity,
        loop_behavior: LoopBehavior,
    },
    ActivatePatternBank {
        bank: PatternBank,
        transition_mode: TransitionMode,
    },
    WriteModulationBuffer {
        bank: ModulationBank,
        offset: u32,
        data: Vec<u8>,
    },
    ConfigModulation {
        bank: ModulationBank,
        config: SamplingConfig,
        size: u32,
        loop_behavior: LoopBehavior,
    },
    ActivateModulationBank {
        bank: ModulationBank,
        transition_mode: TransitionMode,
    },
    Clear,
    Synchronize,
    ReleaseFailsafe,
    Nop,
    ForceFan(bool),
    SetCpuConfig(Box<CpuConfig>),
    SetFpgaConfig(FpgaConfig),
    SetSilencerCompletion {
        intensity: Duration,
        phase: Duration,
        strict: bool,
    },
    SetSilencerUpdateRate {
        intensity: NonZeroU16,
        phase: NonZeroU16,
    },
    SetSilencerDisable,
    SetGpioOut([GpioOut; 4]),
    EmulateGpioIn([bool; 4]),
    SetOutputMask(SetOutputMask),
    SetPhaseCorrection(SetPhaseCorrection),
    SetPulseWidthTable(Box<SetPulseWidthTable>),
    FociStm {
        config: SamplingConfig,
        points: Vec<ControlPoints>,
        bank: PatternBank,
        sound_speed: f32,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    },
    PatternStm {
        config: SamplingConfig,
        phases: Vec<Vec<Vec<Phase>>>,
        intensities: OwnedStmIntensity,
        bank: PatternBank,
        phase_depth: PhaseDepth,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    },
}

pub const AUTD3_STM_INTENSITY_UNIFORM: u8 = 0;
pub const AUTD3_STM_INTENSITY_SAME_FOR_ALL_INDICES: u8 = 1;
pub const AUTD3_STM_INTENSITY_PER_INDEX: u8 = 2;

unsafe fn owned_stm_intensity(
    kind: u8,
    intensities: *const *const IntensityBuffer,
    num_intensities: usize,
    uniform_intensity: u8,
) -> Option<OwnedStmIntensity> {
    match kind {
        AUTD3_STM_INTENSITY_UNIFORM => {
            Some(OwnedStmIntensity::Uniform(Intensity(uniform_intensity)))
        }
        AUTD3_STM_INTENSITY_SAME_FOR_ALL_INDICES if num_intensities == 1 => {
            let buffers = unsafe { clone_buffers(intensities, num_intensities) }?;
            buffers
                .into_iter()
                .next()
                .map(OwnedStmIntensity::SameForAllIndices)
        }
        AUTD3_STM_INTENSITY_PER_INDEX => {
            unsafe { clone_buffers(intensities, num_intensities) }.map(OwnedStmIntensity::PerIndex)
        }
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_pattern(
    bank: u8,
    phases: *const PhaseBuffer,
    intensities: *const IntensityBuffer,
    uniform_intensity: u8,
    transition_mode: u8,
    transition_value: u64,
) -> *mut OwnedCommand {
    unsafe fn owned_pattern_intensity(
        intensities: *const IntensityBuffer,
        uniform_intensity: u8,
    ) -> Option<OwnedPatternIntensity> {
        if intensities.is_null() {
            return Some(OwnedPatternIntensity::Uniform(Intensity(uniform_intensity)));
        }
        unsafe { handle_ref(intensities) }
            .map(|intensities| OwnedPatternIntensity::PerDevice(intensities.0.clone()))
    }

    let Some(phases) = (unsafe { handle_ref(phases) }) else {
        return std::ptr::null_mut();
    };
    let Some(intensities) = (unsafe { owned_pattern_intensity(intensities, uniform_intensity) })
    else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        TransitionMode::from_parts(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };

    into_handle(OwnedCommand::Pattern {
        phases: phases.0.clone(),
        intensities,
        bank,
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_modulation(
    bank: u8,
    sampling_divide: u16,
    modulation_buffer: *const ModulationBuffer,
    loop_rep: u16,
    transition_mode: u8,
    transition_value: u64,
) -> *mut OwnedCommand {
    let (Some(sampling_config), Some(modulation_buffer)) =
        (to_sampling_config(sampling_divide), unsafe {
            handle_ref(modulation_buffer)
        })
    else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_modulation_bank(bank),
        TransitionMode::from_parts(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };

    let data = modulation_buffer.0.clone();
    into_handle(OwnedCommand::Modulation {
        config: sampling_config,
        data,
        bank,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_pattern_buffer(
    bank: u8,
    start_index: u16,
    phases: *const *const PhaseBuffer,
    num_patterns: usize,
    intensity_kind: u8,
    intensities: *const *const IntensityBuffer,
    num_intensities: usize,
    uniform_intensity: u8,
) -> *mut OwnedCommand {
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };
    let Some(patterns) = (unsafe { clone_buffers(phases, num_patterns) }) else {
        return std::ptr::null_mut();
    };
    let Some(intensities) = (unsafe {
        owned_stm_intensity(
            intensity_kind,
            intensities,
            num_intensities,
            uniform_intensity,
        )
    }) else {
        return std::ptr::null_mut();
    };

    into_handle(OwnedCommand::WritePatternBuffer {
        bank,
        start_index,
        patterns,
        intensities,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_write_pattern_buffer_max_len() -> usize {
    WritePatternBuffer::MAX_LEN
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_foci_buffer(
    bank: u8,
    start_index: u32,
    points: *const Autd3StmControlPoint,
    num_samples: usize,
    num_foci: u8,
    intensities: *const u8,
) -> *mut OwnedCommand {
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };

    let Some(points) = (unsafe { foci_points(points, num_samples, num_foci, intensities) }) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::WriteFociBuffer {
        bank,
        start_index: start_index as usize,
        points,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_pattern_phase(
    bank: u8,
    start_index: u16,
    depth: u8,
    intensity: u8,
    patterns: *const *const PhaseBuffer,
    num_patterns: usize,
) -> *mut OwnedCommand {
    let (Some(bank), Some(depth)) = (to_pattern_bank(bank), PhaseDepth::from_u8(depth)) else {
        return std::ptr::null_mut();
    };
    let Some(patterns) = (unsafe { clone_buffers(patterns, num_patterns) }) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::WritePatternPhase {
        bank,
        start_index,
        depth,
        intensity: Intensity(intensity),
        patterns,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_phase_depth_max_count(depth: u8, out: *mut usize) -> i32 {
    let Some(depth) = PhaseDepth::from_u8(depth) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, depth.max_count()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_config_pattern(
    bank: u8,
    sampling_divide: u16,
    size: u32,
    rep: u16,
) -> *mut OwnedCommand {
    let Some(sampling_config) = to_sampling_config(sampling_divide) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::ConfigPattern {
        bank,
        config: sampling_config,
        size,
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_config_foci_stm(
    bank: u8,
    sampling_divide: u16,
    size: u32,
    num_foci: u8,
    sound_speed_m_s: f32,
    rep: u16,
) -> *mut OwnedCommand {
    let Some(sampling_config) = to_sampling_config(sampling_divide) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::ConfigFociStm {
        bank,
        config: sampling_config,
        size,
        num_foci,
        sound_speed: Velocity::from_m_s(sound_speed_m_s),
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_activate_pattern_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
) -> *mut OwnedCommand {
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        TransitionMode::from_parts(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::ActivatePatternBank {
        bank,
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_modulation_buffer(
    bank: u8,
    offset: u32,
    modulation_buffer: *const ModulationBuffer,
) -> *mut OwnedCommand {
    let Some(modulation_buffer) = (unsafe { handle_ref(modulation_buffer) }) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_modulation_bank(bank) else {
        return std::ptr::null_mut();
    };

    let data = modulation_buffer.0.clone();
    into_handle(OwnedCommand::WriteModulationBuffer { bank, offset, data })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_write_modulation_buffer_max_len() -> usize {
    WriteModulationBuffer::MAX_LEN
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_config_modulation(
    bank: u8,
    sampling_divide: u16,
    size: u32,
    rep: u16,
) -> *mut OwnedCommand {
    let Some(sampling_config) = to_sampling_config(sampling_divide) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_modulation_bank(bank) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::ConfigModulation {
        bank,
        config: sampling_config,
        size,
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_activate_modulation_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
) -> *mut OwnedCommand {
    let (Some(bank), Some(transition_mode)) = (
        to_modulation_bank(bank),
        TransitionMode::from_parts(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::ActivateModulationBank {
        bank,
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_clear() -> *mut OwnedCommand {
    into_handle(OwnedCommand::Clear)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_synchronize() -> *mut OwnedCommand {
    into_handle(OwnedCommand::Synchronize)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_release_failsafe() -> *mut OwnedCommand {
    into_handle(OwnedCommand::ReleaseFailsafe)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_nop() -> *mut OwnedCommand {
    into_handle(OwnedCommand::Nop)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_force_fan(value: bool) -> *mut OwnedCommand {
    into_handle(OwnedCommand::ForceFan(value))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Autd3PtpConfig {
    pub sync_interval_ns: u64,
    pub tx_timestamp_timeout_ns: u64,
    pub delay_resp_timeout_ns: u64,
    pub holdover_ns: u64,
    pub lock_samples: u16,
    pub step_threshold_ns: u64,
    pub lock_threshold_ns: u64,
    pub kp_milli: u32,
    pub ki_milli: u32,
    pub max_freq_ppb: u32,
    pub delay_req_syncs: u16,
    pub path_delay_filter_shift: u8,
    pub pause_quanta: u16,
    pub pause_hold_syncs: u16,
    pub pause_retry_ns: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Autd3CpuConfig {
    pub sync_guard_ns: u64,
    pub has_failsafe_timeout: bool,
    pub failsafe_timeout_ns: u64,
    pub has_ptp_unlock_failsafe_timeout: bool,
    pub ptp_unlock_failsafe_timeout_ns: u64,
    pub fpga_bus_wait: u8,
    pub has_cpu_watchdog_timeout: bool,
    pub cpu_watchdog_timeout_ns: u64,
    pub ptp: Autd3PtpConfig,
}

impl From<CpuConfig> for Autd3CpuConfig {
    fn from(config: CpuConfig) -> Self {
        let ptp = config.ptp;
        Self {
            sync_guard_ns: to_ns(config.sync_guard),
            has_failsafe_timeout: config.failsafe_timeout.is_some(),
            failsafe_timeout_ns: config.failsafe_timeout.map_or(0, to_ns),
            has_ptp_unlock_failsafe_timeout: config.ptp_unlock_failsafe_timeout.is_some(),
            ptp_unlock_failsafe_timeout_ns: config.ptp_unlock_failsafe_timeout.map_or(0, to_ns),
            fpga_bus_wait: config.fpga_bus_wait.as_u8(),
            has_cpu_watchdog_timeout: config.cpu_watchdog_timeout.is_some(),
            cpu_watchdog_timeout_ns: config.cpu_watchdog_timeout.map_or(0, to_ns),
            ptp: Autd3PtpConfig {
                sync_interval_ns: to_ns(ptp.sync_interval),
                tx_timestamp_timeout_ns: to_ns(ptp.tx_timestamp_timeout),
                delay_resp_timeout_ns: to_ns(ptp.delay_resp_timeout),
                holdover_ns: to_ns(ptp.holdover),
                lock_samples: ptp.lock_samples.get(),
                step_threshold_ns: to_ns(ptp.step_threshold),
                lock_threshold_ns: to_ns(ptp.lock_threshold),
                kp_milli: ptp.kp_milli,
                ki_milli: ptp.ki_milli,
                max_freq_ppb: ptp.max_freq_ppb,
                delay_req_syncs: ptp.delay_req_syncs.get(),
                path_delay_filter_shift: ptp.path_delay_filter_shift,
                pause_quanta: ptp.pause_quanta.map_or(0, NonZeroU16::get),
                pause_hold_syncs: ptp.pause_hold_syncs,
                pause_retry_ns: to_ns(ptp.pause_retry),
            },
        }
    }
}

fn out_of_range(field: &str, value: impl std::fmt::Display) -> String {
    format!("{field} is out of range: {value}")
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_cpu_config_default(out: *mut Autd3CpuConfig) -> i32 {
    unsafe { write_out(out, CpuConfig::default().into()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_cpu_config(
    config: Autd3CpuConfig,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut OwnedCommand {
    fn to_cpu_config(config: Autd3CpuConfig) -> Result<CpuConfig, String> {
        let optional = |has: bool, ns: u64| has.then(|| Duration::from_nanos(ns));
        let ptp = config.ptp;
        Ok(CpuConfig {
            sync_guard: Duration::from_nanos(config.sync_guard_ns),
            failsafe_timeout: optional(config.has_failsafe_timeout, config.failsafe_timeout_ns),
            ptp_unlock_failsafe_timeout: optional(
                config.has_ptp_unlock_failsafe_timeout,
                config.ptp_unlock_failsafe_timeout_ns,
            ),
            fpga_bus_wait: FpgaBusWait::from_u8(config.fpga_bus_wait)
                .ok_or_else(|| out_of_range("fpga_bus_wait", config.fpga_bus_wait))?,
            cpu_watchdog_timeout: optional(
                config.has_cpu_watchdog_timeout,
                config.cpu_watchdog_timeout_ns,
            ),
            ptp: PtpConfig {
                sync_interval: Duration::from_nanos(ptp.sync_interval_ns),
                tx_timestamp_timeout: Duration::from_nanos(ptp.tx_timestamp_timeout_ns),
                delay_resp_timeout: Duration::from_nanos(ptp.delay_resp_timeout_ns),
                holdover: Duration::from_nanos(ptp.holdover_ns),
                lock_samples: NonZeroU16::new(ptp.lock_samples)
                    .ok_or_else(|| out_of_range("ptp.lock_samples", ptp.lock_samples))?,
                step_threshold: Duration::from_nanos(ptp.step_threshold_ns),
                lock_threshold: Duration::from_nanos(ptp.lock_threshold_ns),
                kp_milli: ptp.kp_milli,
                ki_milli: ptp.ki_milli,
                max_freq_ppb: ptp.max_freq_ppb,
                delay_req_syncs: NonZeroU16::new(ptp.delay_req_syncs)
                    .ok_or_else(|| out_of_range("ptp.delay_req_syncs", ptp.delay_req_syncs))?,
                path_delay_filter_shift: ptp.path_delay_filter_shift,
                pause_quanta: NonZeroU16::new(ptp.pause_quanta),
                pause_hold_syncs: ptp.pause_hold_syncs,
                pause_retry: Duration::from_nanos(ptp.pause_retry_ns),
            },
        })
    }

    match to_cpu_config(config) {
        Ok(config) => into_handle(OwnedCommand::SetCpuConfig(Box::new(config))),
        Err(message) => {
            unsafe { write_cstr(out_err, out_err_len, &message) };
            std::ptr::null_mut()
        }
    }
}

#[repr(C)]
pub struct Autd3FpgaConfig {
    pub sync_phase_lock: bool,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_fpga_config_default(out: *mut Autd3FpgaConfig) -> i32 {
    let config = FpgaConfig::default();
    unsafe {
        write_out(
            out,
            Autd3FpgaConfig {
                sync_phase_lock: config.sync_phase_lock,
            },
        )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_fpga_config(config: Autd3FpgaConfig) -> *mut OwnedCommand {
    into_handle(OwnedCommand::SetFpgaConfig(FpgaConfig {
        sync_phase_lock: config.sync_phase_lock,
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_silencer_completion_time(
    intensity_ns: u64,
    phase_ns: u64,
    strict: bool,
) -> *mut OwnedCommand {
    into_handle(OwnedCommand::SetSilencerCompletion {
        intensity: Duration::from_nanos(intensity_ns),
        phase: Duration::from_nanos(phase_ns),
        strict,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_silencer_update_rate(
    intensity: u16,
    phase: u16,
) -> *mut OwnedCommand {
    let (Some(intensity), Some(phase)) = (NonZeroU16::new(intensity), NonZeroU16::new(phase))
    else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::SetSilencerUpdateRate { intensity, phase })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_silencer_disable() -> *mut OwnedCommand {
    into_handle(OwnedCommand::SetSilencerDisable)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_gpio_out(outputs: *const Autd3GpioOut) -> *mut OwnedCommand {
    let Some(outputs) = (unsafe { slice_ref(outputs, 4) }) else {
        return std::ptr::null_mut();
    };

    let (Some(o0), Some(o1), Some(o2), Some(o3)) = (
        to_gpio_out(&outputs[0]),
        to_gpio_out(&outputs[1]),
        to_gpio_out(&outputs[2]),
        to_gpio_out(&outputs[3]),
    ) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::SetGpioOut([o0, o1, o2, o3]))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_emulate_gpio_in(values: *const u8) -> *mut OwnedCommand {
    let Some(values) = (unsafe { slice_ref(values, 4) }) else {
        return std::ptr::null_mut();
    };

    into_handle(OwnedCommand::EmulateGpioIn([
        values[0] != 0,
        values[1] != 0,
        values[2] != 0,
        values[3] != 0,
    ]))
}

fn total_len(lens: &[usize]) -> Option<usize> {
    lens.iter()
        .try_fold(0usize, |acc, &len| acc.checked_add(len))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_output_mask(
    masks: *const u8,
    lens: *const usize,
    num_devices: usize,
) -> *mut OwnedCommand {
    unsafe fn per_device<T>(
        values: *const u8,
        lens: *const usize,
        num_devices: usize,
        convert: impl Fn(u8) -> T,
    ) -> Option<Vec<Vec<T>>> {
        let lens = unsafe { slice_ref(lens, num_devices) }?;
        let mut rest = unsafe { slice_ref(values, total_len(lens)?) }?;
        Some(
            lens.iter()
                .map(|&len| {
                    let (device, tail) = rest.split_at(len);
                    rest = tail;
                    device.iter().map(|&v| convert(v)).collect()
                })
                .collect(),
        )
    }

    match unsafe { per_device(masks, lens, num_devices, |v| v != 0) } {
        Some(masks) => into_handle(OwnedCommand::SetOutputMask(SetOutputMask { masks })),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_phase_correction(
    phases: *const PhaseBuffer,
) -> *mut OwnedCommand {
    match unsafe { handle_ref(phases) } {
        Some(phases) => into_handle(OwnedCommand::SetPhaseCorrection(SetPhaseCorrection {
            phases: phases.0.clone(),
        })),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_pulse_width_table(table: *const u16) -> *mut OwnedCommand {
    let Some(slice) = (unsafe { slice_ref(table, PWE_TABLE_SIZE) }) else {
        return std::ptr::null_mut();
    };

    let mut op = Box::new(SetPulseWidthTable {
        table: SetPulseWidthTable::empty_table(),
    });
    for (dst, &src) in op.table.iter_mut().zip(slice.iter()) {
        let Ok(value) = PulseWidth::new(src) else {
            return std::ptr::null_mut();
        };
        *dst = value;
    }
    into_handle(OwnedCommand::SetPulseWidthTable(op))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_pulse_width_table_default() -> *mut OwnedCommand {
    into_handle(OwnedCommand::SetPulseWidthTable(Box::default()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_new(
    raw: u16,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    match PulseWidth::new(raw) {
        Ok(_) => AUTD3_OK,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            AUTD3_ERR_INVALID_ARGUMENT
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_from_duty(
    duty: f32,
    out: *mut u16,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    match PulseWidth::from_duty(duty) {
        Ok(value) => unsafe { write_out(out, value.pulse_width()) },
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            AUTD3_ERR_INVALID_ARGUMENT
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_foci_stm(
    config_divide: u16,
    points: *const Autd3StmControlPoint,
    num_samples: usize,
    num_foci: u8,
    intensities: *const u8,
    bank: u8,
    sound_speed_m_s: f32,
    loop_rep: u16,
    transition_mode: u8,
    transition_value: u64,
) -> *mut OwnedCommand {
    let Some(config) = to_sampling_config(config_divide) else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        TransitionMode::from_parts(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };

    let Some(points) = (unsafe { foci_points(points, num_samples, num_foci, intensities) }) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::FociStm {
        config,
        points,
        bank,
        sound_speed: sound_speed_m_s,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_pattern_stm(
    config_divide: u16,
    phases: *const *const PhaseBuffer,
    num_patterns: usize,
    intensity_kind: u8,
    intensities: *const *const IntensityBuffer,
    num_intensities: usize,
    uniform_intensity: u8,
    bank: u8,
    phase_depth: u8,
    loop_rep: u16,
    transition_mode: u8,
    transition_value: u64,
) -> *mut OwnedCommand {
    let Some(config) = to_sampling_config(config_divide) else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(phase_depth), Some(transition_mode)) = (
        to_pattern_bank(bank),
        PhaseDepth::from_u8(phase_depth),
        TransitionMode::from_parts(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };

    let Some(phases) = (unsafe { clone_buffers(phases, num_patterns) }) else {
        return std::ptr::null_mut();
    };
    let Some(intensities) = (unsafe {
        owned_stm_intensity(
            intensity_kind,
            intensities,
            num_intensities,
            uniform_intensity,
        )
    }) else {
        return std::ptr::null_mut();
    };
    into_handle(OwnedCommand::PatternStm {
        config,
        phases,
        intensities,
        bank,
        phase_depth,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_free(op: *mut OwnedCommand) {
    unsafe { drop_handle(op) }
}

impl OwnedCommand {
    fn check_each(&self, num_devices: usize) -> Result<(), String> {
        match self {
            OwnedCommand::Each(devices) if devices.len() != num_devices => Err(format!(
                "per-device command holds {} entries for {num_devices} devices",
                devices.len()
            )),
            OwnedCommand::Each(devices) => devices
                .iter()
                .enumerate()
                .filter_map(|(device, op)| op.as_ref().map(|op| (device, op)))
                .try_for_each(|(device, op)| match op.operation() {
                    Some(_) => Ok(()),
                    None => Err(format!(
                        "per-device command entry for device {device} is not a single-frame operation"
                    )),
                }),
            _ => Ok(()),
        }
    }

    fn boxed(&self) -> BoxedCommand<'_> {
        match self.lower() {
            Lowered::Operation(op) => op.boxed(),
            Lowered::Command(command) => command,
        }
    }

    fn operation(&self) -> Option<Box<dyn Operation + '_>> {
        match self.lower() {
            Lowered::Operation(op) => Some(op),
            Lowered::Command(_) => None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn lower(&self) -> Lowered<'_> {
        match self {
            OwnedCommand::Each(devices) => Lowered::Command(
                each(|device| {
                    devices
                        .get(device.idx())
                        .and_then(Option::as_ref)
                        .and_then(OwnedCommand::operation)
                })
                .boxed(),
            ),
            OwnedCommand::Pattern {
                phases,
                intensities,
                bank,
                transition_mode,
            } => Lowered::Operation(Box::new(Pattern {
                transition_mode: *transition_mode,
                ..Pattern::with_bank(*bank, phases, intensities.as_ref())
            })),
            OwnedCommand::Modulation {
                config,
                data,
                bank,
                loop_behavior,
                transition_mode,
            } => Lowered::Command(
                Modulation {
                    bank: *bank,
                    config: *config,
                    data,
                    loop_behavior: *loop_behavior,
                    transition_mode: *transition_mode,
                }
                .boxed(),
            ),
            OwnedCommand::WritePatternBuffer {
                bank,
                start_index,
                patterns,
                intensities,
            } => Lowered::Operation(Box::new(WritePatternBuffer::new(
                *bank,
                usize::from(*start_index),
                patterns,
                intensities.as_ref(),
            ))),
            OwnedCommand::ConfigPattern {
                bank,
                config,
                size,
                loop_behavior,
            } => Lowered::Operation(Box::new(ConfigPattern {
                bank: *bank,
                config: *config,
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                loop_behavior: *loop_behavior,
            })),
            OwnedCommand::ConfigFociStm {
                bank,
                config,
                size,
                num_foci,
                sound_speed,
                loop_behavior,
            } => Lowered::Operation(Box::new(ConfigFociStm {
                bank: *bank,
                config: *config,
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                num_foci: *num_foci,
                sound_speed: *sound_speed,
                loop_behavior: *loop_behavior,
            })),
            OwnedCommand::ActivatePatternBank {
                bank,
                transition_mode,
            } => Lowered::Operation(Box::new(ActivatePatternBank {
                bank: *bank,
                transition_mode: *transition_mode,
            })),
            OwnedCommand::WriteModulationBuffer { bank, offset, data } => {
                Lowered::Operation(Box::new(WriteModulationBuffer {
                    bank: *bank,
                    offset: usize::try_from(*offset).unwrap_or(usize::MAX),
                    data,
                }))
            }
            OwnedCommand::ConfigModulation {
                bank,
                config,
                size,
                loop_behavior,
            } => Lowered::Operation(Box::new(ConfigModulation {
                bank: *bank,
                config: *config,
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                loop_behavior: *loop_behavior,
            })),
            OwnedCommand::ActivateModulationBank {
                bank,
                transition_mode,
            } => Lowered::Operation(Box::new(ActivateModulationBank {
                bank: *bank,
                transition_mode: *transition_mode,
            })),
            OwnedCommand::Clear => Lowered::Operation(Box::new(Clear)),
            OwnedCommand::Synchronize => Lowered::Operation(Box::new(Synchronize)),
            OwnedCommand::ReleaseFailsafe => Lowered::Operation(Box::new(ReleaseFailsafe)),
            OwnedCommand::Nop => Lowered::Operation(Box::new(Nop)),
            OwnedCommand::ForceFan(value) => {
                Lowered::Operation(Box::new(ForceFan { value: *value }))
            }
            OwnedCommand::SetCpuConfig(config) => {
                Lowered::Operation(Box::new(SetCpuConfig::new(**config)))
            }
            OwnedCommand::SetFpgaConfig(config) => {
                Lowered::Operation(Box::new(SetFpgaConfig::new(*config)))
            }
            OwnedCommand::SetSilencerCompletion {
                intensity,
                phase,
                strict,
            } => Lowered::Operation(Box::new(SetSilencer::new(FixedCompletionTime {
                intensity: *intensity,
                phase: *phase,
                strict_mode: *strict,
            }))),
            OwnedCommand::SetSilencerUpdateRate { intensity, phase } => {
                Lowered::Operation(Box::new(SetSilencer::new(FixedUpdateRate {
                    intensity: *intensity,
                    phase: *phase,
                })))
            }
            OwnedCommand::SetSilencerDisable => {
                Lowered::Operation(Box::new(SetSilencer::disable()))
            }
            OwnedCommand::SetGpioOut(outputs) => {
                Lowered::Operation(Box::new(SetGpioOut { outputs: *outputs }))
            }
            OwnedCommand::EmulateGpioIn(values) => {
                Lowered::Operation(Box::new(EmulateGpioIn { values: *values }))
            }
            OwnedCommand::SetOutputMask(op) => Lowered::Operation(Box::new(op)),
            OwnedCommand::SetPhaseCorrection(op) => Lowered::Operation(Box::new(op)),
            OwnedCommand::SetPulseWidthTable(op) => Lowered::Operation(Box::new(&**op)),
            OwnedCommand::WriteFociBuffer {
                bank,
                start_index,
                points,
            } => Lowered::Operation(Box::new(WriteFociBuffer {
                bank: *bank,
                start_index: *start_index,
                points,
            })),
            OwnedCommand::WritePatternPhase {
                bank,
                start_index,
                depth,
                intensity,
                patterns,
            } => Lowered::Operation(Box::new(WritePatternPhase {
                bank: *bank,
                start_index: usize::from(*start_index),
                depth: *depth,
                intensity: *intensity,
                patterns,
            })),
            OwnedCommand::FociStm {
                config,
                points,
                bank,
                sound_speed,
                loop_behavior,
                transition_mode,
            } => Lowered::Command(
                CoreFociStm::new(
                    *config,
                    points,
                    FociStmOption {
                        bank: *bank,
                        sound_speed: Velocity::from_m_s(*sound_speed),
                        loop_behavior: *loop_behavior,
                        transition_mode: *transition_mode,
                    },
                )
                .boxed(),
            ),
            OwnedCommand::PatternStm {
                config,
                phases,
                intensities,
                bank,
                phase_depth,
                loop_behavior,
                transition_mode,
            } => Lowered::Command(
                PatternStm::new(
                    *config,
                    phases,
                    intensities.as_ref(),
                    PatternStmOption {
                        bank: *bank,
                        phase_depth: *phase_depth,
                        loop_behavior: *loop_behavior,
                        transition_mode: *transition_mode,
                    },
                )
                .boxed(),
            ),
        }
    }
}

enum Lowered<'a> {
    Operation(Box<dyn Operation + 'a>),
    Command(BoxedCommand<'a>),
}

fn is_distinct(ops: &[*mut OwnedCommand]) -> bool {
    ops.iter()
        .enumerate()
        .all(|(i, p)| p.is_null() || !ops[..i].contains(p))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_command_each(
    ops: *const *mut OwnedCommand,
    num_devices: usize,
) -> *mut OwnedCommand {
    let Some(ops) = (unsafe { slice_ref(ops, num_devices) }) else {
        return std::ptr::null_mut();
    };
    if !is_distinct(ops) {
        return std::ptr::null_mut();
    }

    into_handle(OwnedCommand::Each(
        ops.iter().map(|&p| unsafe { take_handle(p) }).collect(),
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_frames_new() -> *mut Arc<Frames> {
    into_handle(Arc::new(Frames::default()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_frames_encode_into(
    frames: *mut Arc<Frames>,
    geometry: *const Geometry,
    command: *mut OwnedCommand,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(frames) = (unsafe { handle_mut(frames) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null frames") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    if Arc::get_mut(frames).is_none() {
        *frames = Arc::new(Frames::default());
    }
    let Some(frames) = Arc::get_mut(frames) else {
        unsafe { write_cstr(out_err, out_err_len, "the frames are shared") };
        return AUTD3_ERR;
    };
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null geometry") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(pending) = (unsafe { handle_ref(command.cast_const()) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null command") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    if let Err(message) = pending.check_each(geometry.num_devices()) {
        unsafe { write_cstr(out_err, out_err_len, &message) };
        return AUTD3_ERR_INVALID_ARGUMENT;
    }

    if let Err(e) = frames.encode_into(geometry, pending.boxed()) {
        unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
        return error_code(&e);
    }
    unsafe { drop_handle(command) };
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_frames_num_frames(frames: *const Arc<Frames>) -> usize {
    let Some(frames) = (unsafe { handle_ref::<Arc<Frames>>(frames) }) else {
        return 0;
    };

    frames.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_frames_free(frames: *mut Arc<Frames>) {
    unsafe { drop_handle(frames) }
}

pub struct ClientHandle(Arc<Client>);

pub struct ResponseToken(ResponseFuture);

pub struct ByteArray(Vec<u8>);

pub struct ResponseHandle(Response);

pub struct FirmwareVersionArray(Vec<Autd3FirmwareVersion>);

pub struct TelemetryArray(Vec<Autd3Telemetry>);

#[repr(C)]
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Autd3DeviceStatus {
    pub state: u8,
    pub ptp_locked: bool,
    pub failsafe_active: bool,
}

pub(crate) fn error_code(e: &Error) -> i32 {
    match e {
        Error::Timeout { .. } => AUTD3_ERR_TIMEOUT,
        Error::DeviceError { .. } | Error::UnexpectedReply { .. } => AUTD3_ERR_DEVICE,
        Error::Network(_) | Error::DeviceLost { .. } => AUTD3_ERR_NETWORK,
        Error::InvalidPayload(_) => AUTD3_ERR_INVALID_ARGUMENT,
        Error::DeviceTimeUnknown | Error::Closed => AUTD3_ERR,
        _ => unreachable!(),
    }
}

fn spawn_completion<T>(
    ctx: CompletionCtx,
    future: impl Future<Output = Result<T, Error>> + Send + 'static,
    into_value: impl FnOnce(T) -> *mut c_void + Send + 'static,
) {
    static EXECUTOR: OnceLock<Executor> = OnceLock::new();
    EXECUTOR.get_or_init(Executor::new).spawn(async move {
        match future.await {
            Ok(value) => ctx.ok(into_value(value)),
            Err(e) => ctx.fail(error_code(&e), &e.to_string()),
        }
    });
}

unsafe fn client_ctx(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) -> Option<(Arc<Client>, CompletionCtx)> {
    let ctx = CompletionCtx::new(cb, user_data)?;
    let Some(client) = (unsafe { handle_ref(client) }) else {
        ctx.err("null client");
        return None;
    };
    Some((Arc::clone(&client.0), ctx))
}

unsafe fn send_ctx(
    client: *const ClientHandle,
    frames: *const Arc<Frames>,
    cb: CompletionCallback,
    user_data: *mut c_void,
) -> Option<(Arc<Client>, Arc<Frames>, CompletionCtx)> {
    let ctx = CompletionCtx::new(cb, user_data)?;
    let (Some(client), Some(frames)) = (unsafe { handle_ref(client) }, unsafe {
        handle_ref::<Arc<Frames>>(frames)
    }) else {
        ctx.err("null argument");
        return None;
    };
    Some((Arc::clone(&client.0), Arc::clone(frames), ctx))
}

unsafe fn send_frame_ctx(
    client: *const ClientHandle,
    frames: *const Arc<Frames>,
    frame: usize,
    cb: CompletionCallback,
    user_data: *mut c_void,
) -> Option<(Arc<Client>, Arc<Frames>, CompletionCtx)> {
    let (client, frames, ctx) = unsafe { send_ctx(client, frames, cb, user_data) }?;
    if frame >= frames.len() {
        ctx.invalid_argument(&format!("frame {frame} out of range"));
        return None;
    }
    Some((client, frames, ctx))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_open(
    geometry: *const Geometry,
    iface_kind: u8,
    iface: *const c_char,
    option: udp::Autd3TransportOption,
    out: *mut *mut ClientHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    if out.is_null() {
        unsafe { write_cstr(out_err, out_err_len, "null argument") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    unsafe { out.write(std::ptr::null_mut()) };
    let Some(geometry) = (unsafe { handle_ref::<Geometry>(geometry) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null argument") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let option = match unsafe { udp::to_transport_option(iface_kind, iface, option) } {
        Ok(option) => option,
        Err(message) => {
            unsafe { write_cstr(out_err, out_err_len, &message) };
            return AUTD3_ERR_INVALID_ARGUMENT;
        }
    };

    match Client::open(geometry, &option) {
        Ok(client) => unsafe { write_out(out, into_handle(ClientHandle(Arc::new(client)))) },
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            error_code(&e)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_device_status(
    client: *const ClientHandle,
    out: *mut Autd3DeviceStatus,
    len: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null client") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let status = match client.0.device_status() {
        Ok(status) => status,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            return error_code(&e);
        }
    };
    if len != status.len() {
        unsafe {
            write_cstr(
                out_err,
                out_err_len,
                &format!(
                    "the buffer holds {len} devices, but {} are open",
                    status.len()
                ),
            );
        };
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    let Some(out) = (unsafe { slice_mut(out, len) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null output buffer") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    for (dst, status) in out.iter_mut().zip(status) {
        *dst = Autd3DeviceStatus {
            state: device_state_code(status.state),
            ptp_locked: status.ptp_locked,
            failsafe_active: status.failsafe_active,
        };
    }
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_num_devices(client: *const ClientHandle) -> usize {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return 0;
    };

    client.0.num_devices()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_device_time_now(
    client: *const ClientHandle,
    out_ns: *mut u64,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null client") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    match client.0.device_time_now() {
        Ok(now) => unsafe { write_out(out_ns, now.sys_time()) },
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            error_code(&e)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_ack_timeout(
    client: *const ClientHandle,
    out_ns: *mut u64,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    unsafe { write_out(out_ns, to_ns(client.0.ack_timeout())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_set_ack_timeout(
    client: *const ClientHandle,
    timeout_ns: u64,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    client.0.set_ack_timeout(Duration::from_nanos(timeout_ns));
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_lost_timeout(
    client: *const ClientHandle,
    out_ns: *mut u64,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    unsafe { write_out(out_ns, to_ns(client.0.lost_timeout())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_set_lost_timeout(
    client: *const ClientHandle,
    timeout_ns: u64,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    client.0.set_lost_timeout(Duration::from_nanos(timeout_ns));
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send_heartbeat(
    client: *const ClientHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null client") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    match client.0.send_heartbeat() {
        Ok(()) => AUTD3_OK,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            error_code(&e)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send_checked(
    client: *const ClientHandle,
    frames: *const Arc<Frames>,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, frames, ctx)) = (unsafe { send_ctx(client, frames, cb, user_data) }) else {
        return;
    };
    spawn_completion(
        ctx,
        async move {
            for frame in frames.iter() {
                client.send_frame(frame).await?.await?.check()?;
            }
            Ok(())
        },
        |()| std::ptr::null_mut(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send(
    client: *const ClientHandle,
    frames: *const Arc<Frames>,
    frame: usize,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, frames, ctx)) =
        (unsafe { send_frame_ctx(client, frames, frame, cb, user_data) })
    else {
        return;
    };
    spawn_completion(
        ctx,
        async move {
            let frame = frames
                .frame(frame)
                .expect("send_frame_ctx checks the index");
            client.send_frame(frame).await
        },
        |future| into_handle(ResponseToken(future)).cast(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send_at(
    client: *const ClientHandle,
    frames: *const Arc<Frames>,
    frame: usize,
    exec: Autd3ExecAt,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, frames, ctx)) =
        (unsafe { send_frame_ctx(client, frames, frame, cb, user_data) })
    else {
        return;
    };
    let Some(exec) = to_exec_at(exec) else {
        ctx.invalid_argument("unknown exec_at kind");
        return;
    };
    spawn_completion(
        ctx,
        async move {
            let frame = frames
                .frame(frame)
                .expect("send_frame_ctx checks the index");
            client.send_frame_at(frame, exec).await
        },
        |future| into_handle(ResponseToken(future)).cast(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_token_await(
    token: *mut ResponseToken,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let token = unsafe { take_handle(token) };
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(ResponseToken(future)) = token else {
        ctx.err("null token");
        return;
    };
    spawn_completion(ctx, future, |response| {
        into_handle(ResponseHandle(response)).cast()
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_num_devices(response: *const ResponseHandle) -> usize {
    unsafe { handle_ref(response) }.map_or(0, |r| r.0.error_codes().len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_error_codes(response: *const ResponseHandle) -> *const u8 {
    unsafe { handle_ref(response) }.map_or(std::ptr::null(), |r| r.0.error_codes().as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_reply_data_of_len(
    response: *const ResponseHandle,
    device: usize,
) -> usize {
    unsafe { handle_ref(response) }.map_or(0, |r| r.0.reply_data_of(device).len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_reply_data_of(
    response: *const ResponseHandle,
    device: usize,
) -> *const u8 {
    unsafe { handle_ref(response) }.map_or(std::ptr::null(), |r| r.0.reply_data_of(device).as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_free(response: *mut ResponseHandle) {
    unsafe { drop_handle(response) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_token_free(token: *mut ResponseToken) {
    unsafe { drop_handle(token) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_check(
    data: *const u8,
    len: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let response = match unsafe { slice_ref(data, len) } {
        Some(data) if !data.is_empty() => Response::from_error_codes(data),
        _ => Response::default(),
    };
    match response.check() {
        Ok(()) => AUTD3_OK,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            error_code(&e)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_firmware_version(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, ctx)) = (unsafe { client_ctx(client, cb, user_data) }) else {
        return;
    };
    spawn_completion(
        ctx,
        async move { client.read_firmware_version().await },
        |versions| {
            let versions = versions
                .into_iter()
                .map(|version| Autd3FirmwareVersion {
                    cpu: [version.cpu.major, version.cpu.minor, version.cpu.patch],
                    fpga: [version.fpga.major, version.fpga.minor, version.fpga.patch],
                    is_emulator: version.is_emulator(),
                    is_supported: version.is_supported(),
                })
                .collect();
            into_handle(FirmwareVersionArray(versions)).cast()
        },
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_firmware_version_array_len(
    array: *const FirmwareVersionArray,
) -> usize {
    unsafe { handle_ref(array) }.map_or(0, |a| a.0.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_firmware_version_array_get(
    array: *const FirmwareVersionArray,
    index: usize,
    out: *mut Autd3FirmwareVersion,
) -> i32 {
    let Some(version) = (unsafe { handle_ref(array) }).and_then(|a| a.0.get(index)) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, *version) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_firmware_version_array_free(array: *mut FirmwareVersionArray) {
    unsafe { drop_handle(array) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_firmware_version_supported_series(
    out_major: *mut u8,
    out_minor: *mut u8,
) -> i32 {
    let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
    if unsafe { write_out(out_major, major) } != AUTD3_OK {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    unsafe { write_out(out_minor, minor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_fpga_state(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, ctx)) = (unsafe { client_ctx(client, cb, user_data) }) else {
        return;
    };
    spawn_completion(
        ctx,
        async move { client.read_fpga_state().await },
        |states| into_handle(ByteArray(states.into_iter().map(|state| state.0).collect())).cast(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_telemetry(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, ctx)) = (unsafe { client_ctx(client, cb, user_data) }) else {
        return;
    };
    spawn_completion(
        ctx,
        async move { client.read_telemetry().await },
        |telemetry| {
            let telemetry = telemetry.into_iter().map(to_telemetry).collect();
            into_handle(TelemetryArray(telemetry)).cast()
        },
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_telemetry_array_len(array: *const TelemetryArray) -> usize {
    unsafe { handle_ref(array) }.map_or(0, |a| a.0.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_telemetry_array_get(
    array: *const TelemetryArray,
    index: usize,
    out: *mut Autd3Telemetry,
) -> i32 {
    let Some(telemetry) = (unsafe { handle_ref(array) }).and_then(|a| a.0.get(index)) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, *telemetry) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_telemetry_array_free(array: *mut TelemetryArray) {
    unsafe { drop_handle(array) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_byte_array_len(array: *const ByteArray) -> usize {
    let Some(array) = (unsafe { handle_ref(array) }) else {
        return 0;
    };

    array.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_byte_array_data(array: *const ByteArray) -> *const u8 {
    let Some(array) = (unsafe { handle_ref(array) }) else {
        return std::ptr::null();
    };

    array.0.as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_byte_array_free(array: *mut ByteArray) {
    unsafe { drop_handle(array) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_close(
    client: *const ClientHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null client") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    match client.0.close() {
        Ok(()) => AUTD3_OK,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            error_code(&e)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_free(client: *mut ClientHandle) {
    unsafe { drop_handle(client) }
}

fn device_state_code(state: DeviceState) -> u8 {
    match state {
        DeviceState::Ready => 0,
        DeviceState::Syncing => 1,
        DeviceState::Lost => 2,
        _ => unreachable!(),
    }
}

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_error(pending: *mut OwnedCommand) -> (i32, String) {
        let geometry = Geometry::new(vec![autd3_rs::geometry::Autd3::default()]);
        let mut err = [0 as c_char; 256];
        let frames = autd3_frames_new();
        let code = unsafe {
            autd3_frames_encode_into(
                frames,
                &raw const geometry,
                pending,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        unsafe { autd3_frames_free(frames) };
        if code == AUTD3_OK {
            return (AUTD3_OK, String::new());
        }
        unsafe { autd3_op_free(pending) };
        let message = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (code, message)
    }

    #[test]
    fn a_per_index_stm_intensity_of_one_buffer_is_not_treated_as_same_for_all_indices() {
        let geometry = Geometry::new(vec![autd3_rs::geometry::Autd3::default()]);
        let phases = autd3_ffi_abi::Buffer(geometry.phase_buffer());
        let intensities = autd3_ffi_abi::Buffer(geometry.intensity_buffer());
        let config = SamplingConfig::FREQ_4K.divide().get();
        let phase_ptrs = [&raw const phases, &raw const phases];
        let intensity_ptrs = [&raw const intensities];
        let encode = |kind: u8| {
            let pending = unsafe {
                autd3_op_pattern_stm(
                    config,
                    phase_ptrs.as_ptr(),
                    phase_ptrs.len(),
                    kind,
                    intensity_ptrs.as_ptr(),
                    intensity_ptrs.len(),
                    0xFF,
                    0,
                    8,
                    0xFFFF,
                    0xFF,
                    0,
                )
            };
            assert!(!pending.is_null());
            pending_error(pending)
        };
        assert_eq!(AUTD3_OK, encode(AUTD3_STM_INTENSITY_SAME_FOR_ALL_INDICES).0);
        let (code, message) = encode(AUTD3_STM_INTENSITY_PER_INDEX);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code, "{message}");
        assert!(
            unsafe {
                autd3_op_pattern_stm(
                    config,
                    phase_ptrs.as_ptr(),
                    phase_ptrs.len(),
                    3,
                    intensity_ptrs.as_ptr(),
                    intensity_ptrs.len(),
                    0xFF,
                    0,
                    8,
                    0xFFFF,
                    0xFF,
                    0,
                )
            }
            .is_null()
        );
    }

    #[test]
    fn a_pulse_width_is_validated_with_the_rust_message() {
        let mut out = 0u16;
        let mut err = [0 as c_char; 256];
        assert_eq!(AUTD3_OK, unsafe {
            autd3_pulse_width_new(256, err.as_mut_ptr(), err.len())
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_pulse_width_new(512, err.as_mut_ptr(), err.len())
        });
        let message = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }.to_string_lossy();
        assert_eq!(message, PulseWidth::new(512).unwrap_err().to_string());
        assert_eq!(AUTD3_OK, unsafe {
            autd3_pulse_width_from_duty(0.5, &raw mut out, err.as_mut_ptr(), err.len())
        });
        assert_eq!(256, out);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_pulse_width_from_duty(1.0, &raw mut out, err.as_mut_ptr(), err.len())
        });
        let table = [0u16; PWE_TABLE_SIZE];
        assert!(!unsafe { autd3_op_set_pulse_width_table(table.as_ptr()) }.is_null());
        let mut out_of_range = [0u16; PWE_TABLE_SIZE];
        out_of_range[3] = 512;
        assert!(unsafe { autd3_op_set_pulse_width_table(out_of_range.as_ptr()) }.is_null());
    }

    #[test]
    fn the_client_constants_match_the_rust_constants() {
        assert_eq!(
            FociStmOption::default().sound_speed.m_s(),
            autd3_foci_stm_option_default_sound_speed()
        );
    }

    fn default_cpu_config() -> Autd3CpuConfig {
        let mut out = std::mem::MaybeUninit::<Autd3CpuConfig>::uninit();
        assert_eq!(AUTD3_OK, unsafe {
            autd3_cpu_config_default(out.as_mut_ptr())
        });
        unsafe { out.assume_init() }
    }

    fn set_cpu_config(config: Autd3CpuConfig) -> Result<CpuConfig, String> {
        let mut err = [0 as c_char; 256];
        let op = unsafe { autd3_op_set_cpu_config(config, err.as_mut_ptr(), err.len()) };
        match unsafe { take_handle(op) } {
            Some(OwnedCommand::SetCpuConfig(config)) => Ok(*config),
            Some(_) => unreachable!(),
            None => Err(unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
                .to_string_lossy()
                .into_owned()),
        }
    }

    #[test]
    fn the_default_cpu_config_matches_the_rust_default() {
        assert_eq!(
            Ok(CpuConfig::default()),
            set_cpu_config(default_cpu_config())
        );
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_cpu_config_default(std::ptr::null_mut())
        });
    }

    #[test]
    fn every_cpu_config_field_reaches_the_rust_config() {
        let config = Autd3CpuConfig {
            sync_guard_ns: 13,
            has_failsafe_timeout: true,
            failsafe_timeout_ns: 25_000_000,
            has_ptp_unlock_failsafe_timeout: true,
            ptp_unlock_failsafe_timeout_ns: 26_000_000,
            fpga_bus_wait: 2,
            has_cpu_watchdog_timeout: true,
            cpu_watchdog_timeout_ns: 27_000_000,
            ptp: Autd3PtpConfig {
                sync_interval_ns: 15_000_000,
                tx_timestamp_timeout_ns: 16_000_000,
                delay_resp_timeout_ns: 17_000_000,
                holdover_ns: 18_000_000,
                lock_samples: 19,
                step_threshold_ns: 20,
                lock_threshold_ns: 21,
                kp_milli: 22,
                ki_milli: 23,
                max_freq_ppb: 24,
                delay_req_syncs: 26,
                path_delay_filter_shift: 4,
                pause_quanta: 27,
                pause_hold_syncs: 28,
                pause_retry_ns: 29_000_000,
            },
        };
        let expected = CpuConfig {
            sync_guard: Duration::from_nanos(13),
            failsafe_timeout: Some(Duration::from_millis(25)),
            ptp_unlock_failsafe_timeout: Some(Duration::from_millis(26)),
            fpga_bus_wait: FpgaBusWait::Cycles2,
            cpu_watchdog_timeout: Some(Duration::from_millis(27)),
            ptp: PtpConfig {
                sync_interval: Duration::from_millis(15),
                tx_timestamp_timeout: Duration::from_millis(16),
                delay_resp_timeout: Duration::from_millis(17),
                holdover: Duration::from_millis(18),
                lock_samples: NonZeroU16::new(19).unwrap(),
                step_threshold: Duration::from_nanos(20),
                lock_threshold: Duration::from_nanos(21),
                kp_milli: 22,
                ki_milli: 23,
                max_freq_ppb: 24,
                delay_req_syncs: NonZeroU16::new(26).unwrap(),
                path_delay_filter_shift: 4,
                pause_quanta: NonZeroU16::new(27),
                pause_hold_syncs: 28,
                pause_retry: Duration::from_millis(29),
            },
        };
        assert_eq!(Ok(expected), set_cpu_config(config));
        assert_eq!(config, Autd3CpuConfig::from(expected));
    }

    #[test]
    fn the_optional_durations_follow_their_presence_flag_and_zero_reaches_the_validation() {
        let default = default_cpu_config();
        assert!(!default.has_failsafe_timeout);
        assert!(!default.has_ptp_unlock_failsafe_timeout);
        assert!(default.has_cpu_watchdog_timeout);
        assert_eq!(1_000_000_000, default.cpu_watchdog_timeout_ns);

        let config = set_cpu_config(Autd3CpuConfig {
            has_failsafe_timeout: true,
            failsafe_timeout_ns: 0,
            has_ptp_unlock_failsafe_timeout: false,
            ptp_unlock_failsafe_timeout_ns: 1,
            has_cpu_watchdog_timeout: false,
            ..default
        })
        .unwrap();
        assert_eq!(Some(Duration::ZERO), config.failsafe_timeout);
        assert_eq!(None, config.ptp_unlock_failsafe_timeout);
        assert_eq!(None, config.cpu_watchdog_timeout);
    }

    #[test]
    fn a_zero_pause_quanta_disables_the_pause() {
        let default = default_cpu_config();
        assert_eq!(48, default.ptp.pause_quanta);
        let config = set_cpu_config(Autd3CpuConfig {
            ptp: Autd3PtpConfig {
                pause_quanta: 0,
                ..default.ptp
            },
            ..default
        })
        .unwrap();
        assert_eq!(None, config.ptp.pause_quanta);
    }

    #[test]
    fn out_of_range_cpu_config_values_are_rejected_with_the_field_name() {
        let default = default_cpu_config();
        assert_eq!(3, default.fpga_bus_wait);
        for invalid in [0, 1, 4, u8::MAX] {
            assert_eq!(
                Err(format!("fpga_bus_wait is out of range: {invalid}")),
                set_cpu_config(Autd3CpuConfig {
                    fpga_bus_wait: invalid,
                    ..default
                })
            );
        }
        let ptp = |ptp: Autd3PtpConfig| set_cpu_config(Autd3CpuConfig { ptp, ..default });
        assert_eq!(
            Err("ptp.lock_samples is out of range: 0".to_owned()),
            ptp(Autd3PtpConfig {
                lock_samples: 0,
                ..default.ptp
            })
        );
        assert_eq!(
            Err("ptp.delay_req_syncs is out of range: 0".to_owned()),
            ptp(Autd3PtpConfig {
                delay_req_syncs: 0,
                ..default.ptp
            })
        );
    }

    #[test]
    fn ptp_values_the_device_refuses_are_passed_through_unchecked() {
        let default = default_cpu_config();
        let config = set_cpu_config(Autd3CpuConfig {
            ptp: Autd3PtpConfig {
                max_freq_ppb: u32::MAX,
                path_delay_filter_shift: u8::MAX,
                ..default.ptp
            },
            ..default
        })
        .unwrap();
        assert_eq!(u32::MAX, config.ptp.max_freq_ppb);
        assert_eq!(u8::MAX, config.ptp.path_delay_filter_shift);
    }

    #[test]
    fn the_default_fpga_config_matches_the_rust_default_and_reaches_the_op() {
        let mut config = Autd3FpgaConfig {
            sync_phase_lock: !FpgaConfig::default().sync_phase_lock,
        };
        assert_eq!(AUTD3_OK, unsafe {
            autd3_fpga_config_default(&raw mut config)
        });
        assert_eq!(
            FpgaConfig::default().sync_phase_lock,
            config.sync_phase_lock
        );
        assert_eq!(
            SetFpgaConfig::default().config.sync_phase_lock,
            config.sync_phase_lock
        );
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_fpga_config_default(std::ptr::null_mut())
        });

        let op = autd3_op_set_fpga_config(Autd3FpgaConfig {
            sync_phase_lock: false,
        });
        assert!(matches!(
            unsafe { take_handle(op) },
            Some(OwnedCommand::SetFpgaConfig(config)) if config == FpgaConfig { sync_phase_lock: false }
        ));
    }

    #[test]
    fn unknown_enum_discriminants_are_rejected() {
        assert!(to_pattern_bank(2).is_none());
        assert!(to_modulation_bank(2).is_none());
        assert!(PhaseDepth::from_u8(0).is_none());
        assert!(PhaseDepth::from_u8(2).is_none());
        assert!(TransitionMode::from_parts(0x03, 0).is_none());
        assert!(to_gpio_out(&Autd3GpioOut { kind: 14, value: 0 }).is_none());
        assert!(to_exec_at(Autd3ExecAt { kind: 3, ns: 0 }).is_none());
    }

    #[test]
    fn exec_at_kinds_map_to_the_rust_variants() {
        assert_eq!(
            to_exec_at(Autd3ExecAt { kind: 0, ns: 7 }),
            Some(ExecAt::Immediate)
        );
        assert_eq!(
            to_exec_at(Autd3ExecAt {
                kind: 1,
                ns: 1_500_000
            }),
            Some(ExecAt::Lead(Duration::from_micros(1_500)))
        );
        assert_eq!(
            to_exec_at(Autd3ExecAt {
                kind: 2,
                ns: 123_456_789
            }),
            Some(ExecAt::At(SysTime::from_nanos(123_456_789)))
        );
    }

    #[test]
    fn telemetry_array_hands_out_every_field_of_every_device() {
        let mut telemetry = Telemetry::default();
        telemetry.processed = 0x1234_5678;
        telemetry.failsafe = 0x0708;
        telemetry.boot_failure = 9;
        telemetry.cpu_watchdog = 10;
        telemetry.exec_late = 0x0B0C_0D0E;
        let array = into_handle(TelemetryArray(vec![
            to_telemetry(Telemetry::default()),
            to_telemetry(telemetry),
        ]));
        assert_eq!(2, unsafe { autd3_telemetry_array_len(array) });

        let mut out = to_telemetry(Telemetry::default());
        assert_eq!(AUTD3_OK, unsafe {
            autd3_telemetry_array_get(array, 1, &raw mut out)
        });
        assert_eq!(out.processed, 0x1234_5678);
        assert_eq!(out.failsafe, 0x0708);
        assert_eq!(out.boot_failure, 9);
        assert_eq!(out.cpu_watchdog, 10);
        assert_eq!(out.exec_late, 0x0B0C_0D0E);
        assert_eq!(out.fifo_drop, 0);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_telemetry_array_get(array, 2, &raw mut out)
        });
        unsafe { autd3_telemetry_array_free(array) };
    }

    #[test]
    fn a_lost_device_is_reported_as_a_network_error() {
        assert_eq!(
            error_code(&Error::DeviceLost { device: 3 }),
            AUTD3_ERR_NETWORK
        );
        assert_eq!(
            error_code(&Error::UnexpectedReply { device: 3 }),
            AUTD3_ERR_DEVICE
        );
    }

    #[test]
    fn device_states_have_stable_codes() {
        assert_eq!(device_state_code(DeviceState::Ready), 0);
        assert_eq!(device_state_code(DeviceState::Syncing), 1);
        assert_eq!(device_state_code(DeviceState::Lost), 2);
    }

    #[test]
    fn the_silencer_default_matches_rust() {
        let expected = FixedCompletionTime::default();
        let (mut intensity, mut phase, mut strict) = (0u64, 0u64, false);
        assert_eq!(AUTD3_OK, unsafe {
            autd3_silencer_default_completion_time(
                &raw mut intensity,
                &raw mut phase,
                &raw mut strict,
            )
        });
        assert_eq!(expected.intensity, Duration::from_nanos(intensity));
        assert_eq!(expected.phase, Duration::from_nanos(phase));
        assert_eq!(expected.strict_mode, strict);
    }

    #[test]
    fn fpga_state_getters_follow_rust_for_every_raw_value() {
        for raw in 0..=u8::MAX {
            let state = FpgaState(raw);
            assert_eq!(
                state.is_thermal_asserted(),
                autd3_fpga_state_is_thermal_asserted(raw)
            );
            assert_eq!(
                state.current_mod_bank() == ModulationBank::B1,
                autd3_fpga_state_current_mod_bank(raw) == 1
            );
            assert_eq!(
                state.current_pattern_bank() == PatternBank::B1,
                autd3_fpga_state_current_pattern_bank(raw) == 1
            );
            assert_eq!(
                state.is_pattern_mode(),
                autd3_fpga_state_is_pattern_mode(raw)
            );
            assert_eq!(
                state.is_pattern_stopped(),
                autd3_fpga_state_is_pattern_stopped(raw)
            );
            assert_eq!(state.is_mod_stopped(), autd3_fpga_state_is_mod_stopped(raw));
            assert_eq!(
                state.is_transition_pending(),
                autd3_fpga_state_is_transition_pending(raw)
            );
        }
    }

    #[test]
    fn a_total_length_that_overflows_is_rejected() {
        assert_eq!(Some(6), total_len(&[1, 2, 3]));
        assert!(total_len(&[usize::MAX, 1]).is_none());
    }

    #[test]
    fn an_overflowing_device_length_yields_a_null_handle() {
        let lens = [usize::MAX, 1usize];
        let values = [0u8; 4];
        assert!(
            unsafe { autd3_op_set_output_mask(values.as_ptr(), lens.as_ptr(), lens.len()) }
                .is_null()
        );
    }

    fn geometry_of(num_devices: usize) -> *mut Geometry {
        into_handle(Geometry::new((0..num_devices).map(|_| {
            autd3_rs::Autd3::new(Point3::origin(), autd3_rs::UnitQuaternion::identity())
        })))
    }

    fn encode(
        geometry: *const Geometry,
        command: *mut OwnedCommand,
    ) -> Result<*mut Arc<Frames>, (i32, String)> {
        let frames = autd3_frames_new();
        let mut err = [0 as c_char; 128];
        let code = unsafe {
            autd3_frames_encode_into(frames, geometry, command, err.as_mut_ptr(), err.len())
        };
        if code == AUTD3_OK {
            return Ok(frames);
        }
        unsafe { autd3_frames_free(frames) };
        Err((
            code,
            unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        ))
    }

    fn num_frames_of(geometry: *const Geometry, command: *mut OwnedCommand) -> usize {
        let frames = encode(geometry, command).unwrap();
        let len = unsafe { autd3_frames_num_frames(frames) };
        unsafe { autd3_frames_free(frames) };
        len
    }

    fn two_entries_for_one_device() -> *mut OwnedCommand {
        let per_device = [autd3_op_nop(), autd3_op_nop()];
        unsafe { autd3_command_each(per_device.as_ptr(), per_device.len()) }
    }

    #[test]
    fn a_length_mismatch_leaves_the_command_handle_with_the_caller() {
        let one = geometry_of(1);
        let two = geometry_of(2);

        let command = two_entries_for_one_device();
        assert_eq!(
            AUTD3_ERR_INVALID_ARGUMENT,
            encode(one, command).unwrap_err().0
        );
        assert_eq!(1, num_frames_of(two, command));

        unsafe { drop_handle(one) };
        unsafe { drop_handle(two) };
    }

    #[test]
    fn a_rejected_encode_leaves_the_command_handle_with_the_caller() {
        let geometry = geometry_of(1);
        let op = autd3_op_clear();

        assert!(encode(std::ptr::null(), op).is_err());
        assert!(encode(geometry, std::ptr::null_mut()).is_err());

        assert_eq!(1, num_frames_of(geometry, op));
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_rejected_encode_reports_the_reason_and_the_error_code() {
        let geometry = geometry_of(1);
        assert_eq!(
            Err((AUTD3_ERR_INVALID_ARGUMENT, "null command".to_owned())),
            encode(geometry, std::ptr::null_mut())
        );
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_command_that_fails_to_encode_stays_with_the_caller_and_yields_no_frames() {
        let empty = geometry_of(0);
        let one = geometry_of(1);
        let command = autd3_op_clear();

        let (code, message) = encode(empty, command).unwrap_err();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_ne!("", message);

        let frames = autd3_frames_new();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_frames_encode_into(frames, empty, command, std::ptr::null_mut(), 0)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_frames_encode_into(frames, one, command, std::ptr::null_mut(), 0)
        });
        assert_eq!(1, unsafe { autd3_frames_num_frames(frames) });

        unsafe { autd3_frames_free(frames) };
        unsafe { drop_handle(empty) };
        unsafe { drop_handle(one) };
    }

    #[test]
    fn the_device_status_of_a_null_client_is_an_invalid_argument() {
        let mut out = [Autd3DeviceStatus::default(); 1];
        let mut err = [0 as c_char; 64];
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_device_status(
                std::ptr::null(),
                out.as_mut_ptr(),
                out.len(),
                err.as_mut_ptr(),
                err.len(),
            )
        });
        assert_eq!(out, [Autd3DeviceStatus::default()]);
    }

    fn write_pattern_buffer_of(
        geometry: *const Geometry,
        num_patterns: usize,
    ) -> *mut OwnedCommand {
        let phases = into_handle(autd3_ffi_abi::Buffer(unsafe { &*geometry }.phase_buffer()));
        let buffers = vec![phases.cast_const(); num_patterns];
        let op = unsafe {
            autd3_op_write_pattern_buffer(
                0,
                0,
                buffers.as_ptr(),
                buffers.len(),
                AUTD3_STM_INTENSITY_UNIFORM,
                std::ptr::null(),
                0,
                0xFF,
            )
        };
        unsafe { drop_handle(phases) };
        op
    }

    #[test]
    fn write_pattern_buffer_takes_up_to_max_len_patterns_in_one_frame() {
        let geometry = geometry_of(1);
        let max_len = autd3_op_write_pattern_buffer_max_len();

        assert_eq!(
            1,
            num_frames_of(geometry, write_pattern_buffer_of(geometry, max_len))
        );
        let too_many = write_pattern_buffer_of(geometry, max_len + 1);
        assert!(encode(geometry, too_many).is_err());
        unsafe { autd3_op_free(too_many) };
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn set_phase_correction_takes_a_phase_buffer_of_the_geometry() {
        let geometry = geometry_of(1);
        let two = geometry_of(2);
        let phases = into_handle(autd3_ffi_abi::Buffer(unsafe { &*geometry }.phase_buffer()));

        assert!(unsafe { autd3_op_set_phase_correction(std::ptr::null()) }.is_null());
        assert_eq!(
            1,
            num_frames_of(geometry, unsafe { autd3_op_set_phase_correction(phases) })
        );
        let mismatched = unsafe { autd3_op_set_phase_correction(phases) };
        unsafe { drop_handle(phases) };
        let (code, message) = encode(two, mismatched).unwrap_err();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code, "{message}");
        unsafe { autd3_op_free(mismatched) };
        unsafe { drop_handle(geometry) };
        unsafe { drop_handle(two) };
    }

    #[test]
    fn a_rejected_each_leaves_every_op_handle_with_the_caller() {
        let op = autd3_op_clear();

        assert!(unsafe { autd3_command_each(std::ptr::null(), 1) }.is_null());

        let aliased = [op, op];
        assert!(unsafe { autd3_command_each(aliased.as_ptr(), aliased.len()) }.is_null());

        let geometry = geometry_of(1);
        let ops = [op];
        let command = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };
        assert!(!command.is_null());
        assert_eq!(1, num_frames_of(geometry, command));
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn an_unsent_composed_command_is_freed_with_its_children() {
        let ops = [autd3_op_clear(), std::ptr::null_mut()];
        let per_device = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };
        assert!(!per_device.is_null());

        unsafe { autd3_op_free(per_device) };
    }

    #[test]
    fn the_device_time_of_a_null_client_is_an_error() {
        let mut ns = 0u64;
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_device_time_now(std::ptr::null(), &raw mut ns, std::ptr::null_mut(), 0)
        });
    }

    #[test]
    fn the_timeouts_of_a_null_client_are_an_error() {
        let mut ns = 0u64;
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_ack_timeout(std::ptr::null(), &raw mut ns)
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_lost_timeout(std::ptr::null(), &raw mut ns)
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_set_ack_timeout(std::ptr::null(), 1)
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_set_lost_timeout(std::ptr::null(), 1)
        });
    }

    #[test]
    fn a_heartbeat_through_a_null_client_is_an_error() {
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_send_heartbeat(std::ptr::null(), std::ptr::null_mut(), 0)
        });
    }

    #[test]
    fn each_takes_one_frame_and_leaves_unassigned_devices_out() {
        let geometry = geometry_of(2);
        let ops = [autd3_op_nop(), std::ptr::null_mut()];
        let command = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };

        assert_eq!(1, num_frames_of(geometry, command));
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn frames_are_re_encoded_in_place_even_while_shared() {
        let geometry = geometry_of(1);
        let frames = autd3_frames_new();
        assert_eq!(0, unsafe { autd3_frames_num_frames(frames) });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_frames_encode_into(frames, geometry, autd3_op_clear(), std::ptr::null_mut(), 0)
        });
        assert_eq!(1, unsafe { autd3_frames_num_frames(frames) });

        let held = Arc::clone(unsafe { handle_ref::<Arc<Frames>>(frames) }.unwrap());
        let buffer = ModulationBuffer(vec![0xFF; 2]);
        let modulation = unsafe { autd3_op_modulation(0, 10, &raw const buffer, 0xFFFF, 0xFF, 0) };
        assert_eq!(AUTD3_OK, unsafe {
            autd3_frames_encode_into(frames, geometry, modulation, std::ptr::null_mut(), 0)
        });
        assert_eq!(3, unsafe { autd3_frames_num_frames(frames) });
        assert_eq!(1, held.len());

        let command = autd3_op_clear();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_frames_encode_into(
                std::ptr::null_mut(),
                geometry,
                command,
                std::ptr::null_mut(),
                0,
            )
        });
        unsafe { autd3_op_free(command) };
        unsafe { autd3_frames_free(frames) };
        unsafe { drop_handle(geometry) };
    }

    fn encode_error(geometry: *const Geometry, command: *mut OwnedCommand) -> String {
        let (_, message) = encode(geometry, command).unwrap_err();
        unsafe { autd3_op_free(command) };
        message
    }

    #[test]
    fn an_each_that_does_not_match_the_device_count_fails_to_encode() {
        let geometry = geometry_of(2);

        let short = [autd3_op_nop()];
        let short = unsafe { autd3_command_each(short.as_ptr(), short.len()) };
        assert!(encode_error(geometry, short).contains("1 entries for 2 devices"));

        let long = [autd3_op_nop(), std::ptr::null_mut(), autd3_op_nop()];
        let long = unsafe { autd3_command_each(long.as_ptr(), long.len()) };
        assert!(encode_error(geometry, long).contains("3 entries for 2 devices"));

        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn an_each_holding_a_command_that_is_not_a_single_frame_operation_fails_to_encode() {
        let geometry = geometry_of(2);
        let not_single_frame = "entry for device 1 is not a single-frame operation";

        let buffer = ModulationBuffer(vec![0xFF; 2]);
        let modulation = unsafe { autd3_op_modulation(0, 10, &raw const buffer, 0xFFFF, 0xFF, 0) };
        let ops = [autd3_op_nop(), modulation];
        let command = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };
        assert!(encode_error(geometry, command).contains(not_single_frame));

        let inner = [autd3_op_nop(), autd3_op_nop()];
        let inner = unsafe { autd3_command_each(inner.as_ptr(), inner.len()) };
        let ops = [std::ptr::null_mut(), inner];
        let command = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };
        assert!(encode_error(geometry, command).contains(not_single_frame));

        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn an_each_takes_the_buffer_writes_as_single_frame_operations() {
        let geometry = geometry_of(2);

        let buffer = ModulationBuffer(vec![0xFF; 2]);
        let point = Autd3StmControlPoint {
            point: [0.0, 0.0, 150.0],
            phase_offset: 0,
        };
        let intensity = 0xFF;
        let ops = [
            unsafe { autd3_op_write_modulation_buffer(0, 0, &raw const buffer) },
            unsafe {
                autd3_op_write_foci_buffer(0, 0, &raw const point, 1, 1, &raw const intensity)
            },
        ];
        let command = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };
        assert_eq!(1, num_frames_of(geometry, command));

        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_buffer_write_longer_than_one_frame_fails_to_encode() {
        let geometry = geometry_of(1);

        let max = autd3_op_write_modulation_buffer_max_len();
        let buffer = ModulationBuffer(vec![0xFF; max + 1]);
        let command = unsafe { autd3_op_write_modulation_buffer(0, 0, &raw const buffer) };
        assert!(encode_error(geometry, command).contains("fit in one frame"));

        let max = autd3_op_write_foci_buffer_max_len(2);
        let points: Vec<_> = (0..(max + 1) * 2)
            .map(|_| Autd3StmControlPoint {
                point: [0.0, 0.0, 150.0],
                phase_offset: 0,
            })
            .collect();
        let intensities = vec![0xFF; max + 1];
        let command = unsafe {
            autd3_op_write_foci_buffer(0, 0, points.as_ptr(), max + 1, 2, intensities.as_ptr())
        };
        assert!(encode_error(geometry, command).contains("fit in one frame"));

        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn the_write_foci_buffer_max_len_shrinks_with_the_number_of_foci() {
        assert_eq!(
            autd3_op_write_foci_buffer_max_len(1),
            WriteFociBuffer::max_len(1)
        );
        assert_eq!(
            autd3_op_write_foci_buffer_max_len(8),
            WriteFociBuffer::max_len(8)
        );
        assert_eq!(autd3_op_write_foci_buffer_max_len(0), 0);
        assert_eq!(autd3_op_write_foci_buffer_max_len(9), 0);
    }

    #[test]
    fn the_frame_count_of_a_null_frames_handle_is_zero() {
        assert_eq!(0, unsafe { autd3_frames_num_frames(std::ptr::null()) });
        unsafe { autd3_frames_free(std::ptr::null_mut()) };
    }

    #[test]
    fn a_null_completion_callback_is_ignored() {
        unsafe { autd3_client_read_fpga_state(std::ptr::null(), None, std::ptr::null_mut()) };
    }

    #[test]
    fn closing_a_null_client_is_an_error() {
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_close(std::ptr::null(), std::ptr::null_mut(), 0)
        });
    }
}
