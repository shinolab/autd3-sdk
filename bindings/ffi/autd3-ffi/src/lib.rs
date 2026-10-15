use std::ffi::{c_char, c_void};
use std::future::Future;
use std::mem::ManuallyDrop;
use std::num::{NonZeroU16, NonZeroU32, NonZeroUsize};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

mod logging;
mod udp;

use autd3_ffi_abi::{
    AUTD3_ERR, AUTD3_ERR_DEVICE, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_ERR_NETWORK, AUTD3_ERR_TIMEOUT,
    AUTD3_ERR_UNSUPPORTED_FIRMWARE, AUTD3_OK, Autd3FirmwareVersion, Autd3Telemetry, Buffer,
    CompletionCallback, CompletionCtx, IntensityBuffer, ModulationBuffer, PhaseBuffer, drop_handle,
    handle_mut, handle_ref, into_handle, slice_mut, slice_ref, take_handle, to_ns, write_cstr,
    write_out,
};
use autd3_rs::commands::{
    ActivateModulationBank, ActivatePatternBank, BoxedCommand, Clear, Command, ConfigFociStm,
    ConfigModulation, ConfigPattern, CpuConfig, EmulateGpioIn, Expansion, FixedCompletionTime,
    FixedUpdateRate, FociStm as CoreFociStm, FociStmOption, ForceFan, FpgaBusWait, GpioOut,
    MaxFreqPpb, Modulation, Nop, PWE_TABLE_SIZE, PathDelayFilterShift, Pattern, PatternIntensity,
    PatternStm, PatternStmOption, PhaseDepth, ReleaseFailsafe, SetCpuConfig, SetGpioOut,
    SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, StmConfig, StmIntensity,
    Synchronize, WriteFociBuffer, WriteModulationBuffer, WritePatternBuffer, WritePatternPhase,
    circle, each, line,
};
use autd3_rs::rt::Executor;
use autd3_rs::udp::StateChecker;
use autd3_rs::units::Hz;
use autd3_rs::value::{
    ControlPoint, ControlPoints, GpioIn, Intensity, LoopBehavior, ModulationBank, Nearest,
    PatternBank, Phase, PulseWidth, SamplingConfig, SysTime, TransitionMode,
};
use autd3_rs::{
    BusStats, Client, ClientConfig, DeviceState, Error, FirmwareVersion, FpgaState, Frames,
    Geometry, Length, Link, NetworkCause, Point3, Response, ResponseFuture, StreamFuture,
    Telemetry, UnitVector3, Vector3, Velocity,
};

pub(crate) fn executor() -> &'static Executor {
    static EXECUTOR: OnceLock<Executor> = OnceLock::new();
    EXECUTOR.get_or_init(Executor::new)
}

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

fn to_gpio_in(v: u8) -> Option<GpioIn> {
    match v {
        0 => Some(GpioIn::I0),
        1 => Some(GpioIn::I1),
        2 => Some(GpioIn::I2),
        3 => Some(GpioIn::I3),
        _ => None,
    }
}

fn to_telemetry(telemetry: Telemetry) -> Autd3Telemetry {
    Autd3Telemetry {
        fifo_drop: telemetry.fifo_drop,
        dedup: telemetry.dedup,
        seq_mismatch: telemetry.seq_mismatch,
        dispatch_error: telemetry.dispatch_error,
        processed: telemetry.processed,
        send_failure: telemetry.send_failure,
        failsafe: telemetry.failsafe,
        ptp_unlock_failsafe: telemetry.ptp_unlock_failsafe,
        sync_resync: telemetry.sync_resync,
        boot_failure: telemetry.boot_failure,
        cpu_watchdog: telemetry.cpu_watchdog,
    }
}

pub(crate) fn to_transition_mode(mode: u8, value: u64) -> Option<TransitionMode> {
    match mode {
        0x00 => Some(TransitionMode::SyncIdx),
        0x01 => Some(TransitionMode::SysTime {
            time: SysTime::from_nanos(value),
        }),
        #[allow(clippy::cast_possible_truncation)]
        0x02 => to_gpio_in(value as u8).map(TransitionMode::Gpio),
        0xF0 => Some(TransitionMode::Ext),
        0xFE => Some(TransitionMode::Later),
        0xFF => Some(TransitionMode::Immediate),
        _ => None,
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

fn to_control_point(p: &Autd3StmControlPoint) -> ControlPoint {
    ControlPoint::new(
        Point3::new(p.point[0], p.point[1], p.point[2]),
        Phase(p.phase_offset),
    )
}

macro_rules! foci_points {
    ($($n:literal => $variant:ident),* $(,)?) => {
        pub enum FociPoints {
            $($variant(Vec<ControlPoints<$n>>)),*
        }

        impl FociPoints {
            fn from_flat(
                points: &[Autd3StmControlPoint],
                intensities: &[u8],
                num_foci: usize,
            ) -> Option<Self> {
                match num_foci {
                    $($n => Some(FociPoints::$variant(
                        points
                            .as_chunks::<$n>()
                            .0
                            .iter()
                            .zip(intensities)
                            .map(|(chunk, &intensity)| {
                                ControlPoints::new(
                                    chunk.each_ref().map(to_control_point),
                                    Intensity(intensity),
                                )
                            })
                            .collect(),
                    )),)*
                    _ => None,
                }
            }

            fn boxed_stm(
                &self,
                config: StmConfig,
                option: FociStmOption,
            ) -> BoxedCommand<'_> {
                match self {
                    $(FociPoints::$variant(v) => {
                        CoreFociStm::new(config, v.as_slice(), option).boxed()
                    })*
                }
            }

            fn boxed_write_foci(
                &self,
                bank: PatternBank,
                index_offset: usize,
            ) -> BoxedCommand<'_> {
                match self {
                    $(FociPoints::$variant(v) => {
                        WriteFociBuffer {
                            bank,
                            index_offset,
                            points: v.as_slice(),
                        }
                        .boxed()
                    })*
                }
            }
        }
    };
}
foci_points!(1 => N1, 2 => N2, 3 => N3, 4 => N4, 5 => N5, 6 => N6, 7 => N7, 8 => N8);

unsafe fn foci_points(
    points: *const Autd3StmControlPoint,
    num_samples: usize,
    num_foci: u8,
    intensities: *const u8,
) -> Option<FociPoints> {
    let n = usize::from(num_foci);
    let points = unsafe { slice_ref(points, num_samples * n) }?;
    let intensities = unsafe { slice_ref(intensities, num_samples) }?;
    FociPoints::from_flat(points, intensities, n)
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

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_freq(hz: f32) -> *mut StmConfig {
    into_handle(StmConfig::new(hz * Hz))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_freq_nearest(hz: f32) -> *mut StmConfig {
    into_handle(StmConfig::new(Nearest(hz * Hz)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_period(period_ns: u64) -> *mut StmConfig {
    into_handle(StmConfig::new(Duration::from_nanos(period_ns)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_period_nearest(period_ns: u64) -> *mut StmConfig {
    into_handle(StmConfig::new(Nearest(Duration::from_nanos(period_ns))))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_sampling(divide: u16) -> *mut StmConfig {
    match NonZeroU16::new(divide) {
        Some(divide) => into_handle(StmConfig::new(SamplingConfig::new(divide))),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_stm_config_into_sampling_config(
    config: *const StmConfig,
    size: usize,
    out: *mut u16,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null stm config") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    if u32::try_from(size).is_err() {
        unsafe {
            write_cstr(
                out_err,
                out_err_len,
                "the number of samples is out of range",
            );
        };
        return AUTD3_ERR_INVALID_ARGUMENT;
    }

    match config.into_sampling_config(size).divide() {
        Ok(value) => unsafe { write_out(out, value.get()) },
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            AUTD3_ERR
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_stm_config_free(config: *mut StmConfig) {
    unsafe { drop_handle(config) }
}

unsafe fn write_control_points(
    points: &[ControlPoints<1>],
    out_points: *mut Autd3StmControlPoint,
    out_intensities: *mut u8,
) -> i32 {
    let (Some(out_points), Some(out_intensities)) =
        (unsafe { slice_mut(out_points, points.len()) }, unsafe {
            slice_mut(out_intensities, points.len())
        })
    else {
        return -1;
    };
    for ((out_point, out_intensity), cp) in out_points.iter_mut().zip(out_intensities).zip(points) {
        let p = cp.points[0];
        *out_point = Autd3StmControlPoint {
            point: [p.point.x, p.point.y, p.point.z],
            phase_offset: p.phase_offset.0,
        };
        *out_intensity = cp.intensity.0;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_stm_circle(
    center: *const f32,
    radius_mm: f32,
    num_points: usize,
    normal: *const f32,
    intensity: u8,
    out_points: *mut Autd3StmControlPoint,
    out_intensities: *mut u8,
) -> i32 {
    let (Some(center), Some(normal)) = (unsafe { slice_ref(center, 3) }, unsafe {
        slice_ref(normal, 3)
    }) else {
        return -1;
    };
    let mut points = Vec::new();
    circle(
        Point3::new(center[0], center[1], center[2]),
        Length::from_mm(radius_mm),
        num_points,
        UnitVector3::new_normalize(Vector3::new(normal[0], normal[1], normal[2])),
        Intensity(intensity),
        &mut points,
    );
    unsafe { write_control_points(&points, out_points, out_intensities) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_stm_line(
    start: *const f32,
    end: *const f32,
    num_points: usize,
    intensity: u8,
    out_points: *mut Autd3StmControlPoint,
    out_intensities: *mut u8,
) -> i32 {
    let (Some(start), Some(end)) = (unsafe { slice_ref(start, 3) }, unsafe { slice_ref(end, 3) })
    else {
        return -1;
    };
    let mut points = Vec::new();
    line(
        Point3::new(start[0], start[1], start[2]),
        Point3::new(end[0], end[1], end[2]),
        num_points,
        Intensity(intensity),
        &mut points,
    );
    unsafe { write_control_points(&points, out_points, out_intensities) }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_client_config_new() -> *mut ClientConfig {
    into_handle(ClientConfig::default())
}

macro_rules! client_config_setter {
    ($set:ident, $field:ident, bool) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(config: *mut ClientConfig, value: bool) -> i32 {
            let Some(config) = (unsafe { handle_mut(config) }) else {
                return AUTD3_ERR_INVALID_ARGUMENT;
            };
            config.$field = value;
            AUTD3_OK
        }
    };
    ($set:ident, $field:ident, attempts) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(config: *mut ClientConfig, value: u32) -> i32 {
            let Some(config) = (unsafe { handle_mut(config) }) else {
                return AUTD3_ERR_INVALID_ARGUMENT;
            };
            config.$field = NonZeroU32::new(value);
            AUTD3_OK
        }
    };
    ($set:ident, $field:ident, $raw:ty, $nonzero:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(config: *mut ClientConfig, value: $raw) -> i32 {
            let (Some(config), Some(value)) =
                (unsafe { handle_mut(config) }, <$nonzero>::new(value))
            else {
                return AUTD3_ERR_INVALID_ARGUMENT;
            };
            config.$field = value;
            AUTD3_OK
        }
    };
}

client_config_setter!(
    autd3_client_config_set_require_supported_firmware,
    require_supported_firmware,
    bool
);
client_config_setter!(
    autd3_client_config_set_resynchronize_on_recovery,
    resynchronize_on_recovery,
    attempts
);
client_config_setter!(
    autd3_client_config_set_release_failsafe_on_recovery,
    release_failsafe_on_recovery,
    attempts
);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_set_ack_timeout_ns(
    config: *mut ClientConfig,
    ns: u64,
) -> i32 {
    let Some(config) = (unsafe { handle_mut(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    config.ack_timeout = Duration::from_nanos(ns);
    AUTD3_OK
}

client_config_setter!(
    autd3_client_config_set_max_inflight,
    max_inflight,
    usize,
    NonZeroUsize
);
client_config_setter!(
    autd3_client_config_set_max_resync_rounds,
    max_resync_rounds,
    u32,
    NonZeroU32
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_get_ack_timeout_ns(
    config: *const ClientConfig,
    out: *mut u64,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, to_ns(config.ack_timeout)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_get_max_inflight(
    config: *const ClientConfig,
    out: *mut usize,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, config.max_inflight.get()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_get_max_resync_rounds(
    config: *const ClientConfig,
    out: *mut u32,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, config.max_resync_rounds.get()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_get_require_supported_firmware(
    config: *const ClientConfig,
    out: *mut bool,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, config.require_supported_firmware) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_get_resynchronize_on_recovery(
    config: *const ClientConfig,
    out: *mut u32,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe {
        write_out(
            out,
            config.resynchronize_on_recovery.map_or(0, NonZeroU32::get),
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_get_release_failsafe_on_recovery(
    config: *const ClientConfig,
    out: *mut u32,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe {
        write_out(
            out,
            config
                .release_failsafe_on_recovery
                .map_or(0, NonZeroU32::get),
        )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_params_max_devices() -> usize {
    autd3_rs::MAX_DEVICES
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_params_pwe_table_size() -> usize {
    PWE_TABLE_SIZE
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_params_server_port() -> u16 {
    autd3_rs::SERVER_PORT
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_params_simulator_port() -> u16 {
    autd3_rs::SIMULATOR_PORT
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

#[unsafe(no_mangle)]
pub extern "C" fn autd3_fpga_state_is_failsafe_active(raw: u8) -> bool {
    FpgaState(raw).is_failsafe_active()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_free(config: *mut ClientConfig) {
    unsafe { drop_handle(config) }
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
    Shared(Vec<Vec<Intensity>>),
    PerIndex(Vec<Vec<Vec<Intensity>>>),
}

impl OwnedStmIntensity {
    fn as_ref(&self) -> StmIntensity<'_> {
        match self {
            OwnedStmIntensity::Uniform(intensity) => StmIntensity::Uniform(*intensity),
            OwnedStmIntensity::Shared(intensities) => StmIntensity::Shared(intensities),
            OwnedStmIntensity::PerIndex(intensities) => StmIntensity::PerIndex(intensities),
        }
    }
}

pub enum Pending {
    Each(Vec<Option<Pending>>),
    Sequence(Vec<Pending>),
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
        index: u16,
        phases: Vec<Vec<Phase>>,
        intensities: OwnedPatternIntensity,
    },
    WriteFociBuffer {
        bank: PatternBank,
        index_offset: usize,
        points: FociPoints,
    },
    WritePatternPhase {
        bank: PatternBank,
        index: u16,
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
    SetOutputMask(Vec<Vec<bool>>),
    SetPhaseCorrection(Vec<Vec<Phase>>),
    SetPulseWidthTable(Box<[PulseWidth; PWE_TABLE_SIZE]>),
    FociStm {
        config: StmConfig,
        points: FociPoints,
        bank: PatternBank,
        sound_speed: f32,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    },
    PatternStm {
        config: StmConfig,
        phases: Vec<Vec<Vec<Phase>>>,
        intensities: OwnedStmIntensity,
        bank: PatternBank,
        phase_depth: PhaseDepth,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    },
}

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

pub const AUTD3_STM_INTENSITY_UNIFORM: u8 = 0;
pub const AUTD3_STM_INTENSITY_SHARED: u8 = 1;
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
        AUTD3_STM_INTENSITY_SHARED if num_intensities == 1 => {
            let buffers = unsafe { clone_buffers(intensities, num_intensities) }?;
            buffers.into_iter().next().map(OwnedStmIntensity::Shared)
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
) -> *mut Pending {
    let Some(phases) = (unsafe { handle_ref(phases) }) else {
        return std::ptr::null_mut();
    };
    let Some(intensities) = (unsafe { owned_pattern_intensity(intensities, uniform_intensity) })
    else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        to_transition_mode(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };

    into_handle(Pending::Pattern {
        phases: phases.0.clone(),
        intensities,
        bank,
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_modulation(
    bank: u8,
    sampling_config: *const SamplingConfig,
    modulation_buffer: *const ModulationBuffer,
    loop_rep: u16,
    transition_mode: u8,
    transition_value: u64,
) -> *mut Pending {
    let (Some(sampling_config), Some(modulation_buffer)) =
        (unsafe { handle_ref(sampling_config) }, unsafe {
            handle_ref(modulation_buffer)
        })
    else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_modulation_bank(bank),
        to_transition_mode(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };

    let data = modulation_buffer.0.clone();
    into_handle(Pending::Modulation {
        config: *sampling_config,
        data,
        bank,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_pattern_buffer(
    bank: u8,
    index: u16,
    phases: *const PhaseBuffer,
    intensities: *const IntensityBuffer,
    uniform_intensity: u8,
) -> *mut Pending {
    let Some(phases) = (unsafe { handle_ref(phases) }) else {
        return std::ptr::null_mut();
    };
    let Some(intensities) = (unsafe { owned_pattern_intensity(intensities, uniform_intensity) })
    else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };

    into_handle(Pending::WritePatternBuffer {
        bank,
        index,
        phases: phases.0.clone(),
        intensities,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_foci_buffer(
    bank: u8,
    index_offset: u32,
    points: *const Autd3StmControlPoint,
    num_samples: usize,
    num_foci: u8,
    intensities: *const u8,
) -> *mut Pending {
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };

    let Some(points) = (unsafe { foci_points(points, num_samples, num_foci, intensities) }) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::WriteFociBuffer {
        bank,
        index_offset: index_offset as usize,
        points,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_pattern_phase(
    bank: u8,
    index: u16,
    depth: u8,
    intensity: u8,
    patterns: *const *const PhaseBuffer,
    num_patterns: usize,
) -> *mut Pending {
    let (Some(bank), Some(depth)) = (to_pattern_bank(bank), PhaseDepth::from_u8(depth)) else {
        return std::ptr::null_mut();
    };
    let Some(patterns) = (unsafe { clone_buffers(patterns, num_patterns) }) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::WritePatternPhase {
        bank,
        index,
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
    sampling_config: *const SamplingConfig,
    size: u32,
    rep: u16,
) -> *mut Pending {
    let Some(sampling_config) = (unsafe { handle_ref(sampling_config) }) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::ConfigPattern {
        bank,
        config: *sampling_config,
        size,
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_config_foci_stm(
    bank: u8,
    sampling_config: *const SamplingConfig,
    size: u32,
    num_foci: u8,
    sound_speed_m_s: f32,
    rep: u16,
) -> *mut Pending {
    let Some(sampling_config) = (unsafe { handle_ref(sampling_config) }) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::ConfigFociStm {
        bank,
        config: *sampling_config,
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
) -> *mut Pending {
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        to_transition_mode(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::ActivatePatternBank {
        bank,
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_modulation_buffer(
    bank: u8,
    offset: u32,
    modulation_buffer: *const ModulationBuffer,
) -> *mut Pending {
    let Some(modulation_buffer) = (unsafe { handle_ref(modulation_buffer) }) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_modulation_bank(bank) else {
        return std::ptr::null_mut();
    };

    let data = modulation_buffer.0.clone();
    into_handle(Pending::WriteModulationBuffer { bank, offset, data })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_config_modulation(
    bank: u8,
    sampling_config: *const SamplingConfig,
    size: u32,
    rep: u16,
) -> *mut Pending {
    let Some(sampling_config) = (unsafe { handle_ref(sampling_config) }) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_modulation_bank(bank) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::ConfigModulation {
        bank,
        config: *sampling_config,
        size,
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_activate_modulation_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
) -> *mut Pending {
    let (Some(bank), Some(transition_mode)) = (
        to_modulation_bank(bank),
        to_transition_mode(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::ActivateModulationBank {
        bank,
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_clear() -> *mut Pending {
    into_handle(Pending::Clear)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_synchronize() -> *mut Pending {
    into_handle(Pending::Synchronize)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_release_failsafe() -> *mut Pending {
    into_handle(Pending::ReleaseFailsafe)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_nop() -> *mut Pending {
    into_handle(Pending::Nop)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_force_fan(value: bool) -> *mut Pending {
    into_handle(Pending::ForceFan(value))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_force_fan_default() -> *mut Pending {
    into_handle(Pending::ForceFan(ForceFan::default().value))
}

pub struct CpuConfigHandle(pub(crate) CpuConfig);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_cpu_config_new() -> *mut CpuConfigHandle {
    into_handle(CpuConfigHandle(CpuConfig::default()))
}

macro_rules! cpu_config_bounded_field {
    ([$($field:tt).+], $ty:ty, $bounded:ty, $set:ident, $get:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(handle: *mut CpuConfigHandle, value: $ty) -> i32 {
            let (Some(config), Some(value)) =
                (unsafe { handle_mut(handle) }, <$bounded>::new(value))
            else {
                return AUTD3_ERR_INVALID_ARGUMENT;
            };
            config.0.$($field).+ = value;
            AUTD3_OK
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(handle: *const CpuConfigHandle, out: *mut $ty) -> i32 {
            let Some(config) = (unsafe { handle_ref(handle) }) else {
                return AUTD3_ERR_INVALID_ARGUMENT;
            };
            unsafe { autd3_ffi_abi::write_out(out, config.0.$($field).+.get()) }
        }
    };
}

autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [sys_time_transition_margin],
    duration,
    autd3_cpu_config_set_sys_time_transition_margin,
    autd3_cpu_config_get_sys_time_transition_margin
);
cpu_config_bounded_field!(
    [fpga_wait_update_max_polls],
    u32,
    NonZeroU32,
    autd3_cpu_config_set_fpga_wait_update_max_polls,
    autd3_cpu_config_get_fpga_wait_update_max_polls
);
cpu_config_bounded_field!(
    [fpga_flash_max_polls],
    u32,
    NonZeroU32,
    autd3_cpu_config_set_fpga_flash_max_polls,
    autd3_cpu_config_get_fpga_flash_max_polls
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [sync_guard],
    duration,
    autd3_cpu_config_set_sync_guard,
    autd3_cpu_config_get_sync_guard
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [update_activate_delay],
    duration,
    autd3_cpu_config_set_update_activate_delay,
    autd3_cpu_config_get_update_activate_delay
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [failsafe_timeout],
    optional_duration,
    autd3_cpu_config_set_failsafe_timeout,
    autd3_cpu_config_get_failsafe_timeout
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp_unlock_failsafe_timeout],
    optional_duration,
    autd3_cpu_config_set_ptp_unlock_failsafe_timeout,
    autd3_cpu_config_get_ptp_unlock_failsafe_timeout
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [cpu_watchdog_timeout],
    optional_duration,
    autd3_cpu_config_set_cpu_watchdog_timeout,
    autd3_cpu_config_get_cpu_watchdog_timeout
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_cpu_config_set_fpga_bus_wait(
    handle: *mut CpuConfigHandle,
    cycles: u8,
) -> i32 {
    let (Some(config), Some(wait)) = (unsafe { handle_mut(handle) }, FpgaBusWait::from_u8(cycles))
    else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    config.0.fpga_bus_wait = wait;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_cpu_config_get_fpga_bus_wait(
    handle: *const CpuConfigHandle,
    out: *mut u8,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { autd3_ffi_abi::write_out(out, config.0.fpga_bus_wait.as_u8()) }
}

autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.sync_interval],
    duration,
    autd3_cpu_config_set_ptp_sync_interval,
    autd3_cpu_config_get_ptp_sync_interval
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.tx_timestamp_timeout],
    duration,
    autd3_cpu_config_set_ptp_tx_timestamp_timeout,
    autd3_cpu_config_get_ptp_tx_timestamp_timeout
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.delay_resp_timeout],
    duration,
    autd3_cpu_config_set_ptp_delay_resp_timeout,
    autd3_cpu_config_get_ptp_delay_resp_timeout
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.holdover],
    duration,
    autd3_cpu_config_set_ptp_holdover,
    autd3_cpu_config_get_ptp_holdover
);
cpu_config_bounded_field!(
    [ptp.lock_samples],
    u16,
    NonZeroU16,
    autd3_cpu_config_set_ptp_lock_samples,
    autd3_cpu_config_get_ptp_lock_samples
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.step_threshold],
    duration,
    autd3_cpu_config_set_ptp_step_threshold,
    autd3_cpu_config_get_ptp_step_threshold
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.lock_threshold],
    duration,
    autd3_cpu_config_set_ptp_lock_threshold,
    autd3_cpu_config_get_ptp_lock_threshold
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.kp_milli],
    u32,
    autd3_cpu_config_set_ptp_kp_milli,
    autd3_cpu_config_get_ptp_kp_milli
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.ki_milli],
    u32,
    autd3_cpu_config_set_ptp_ki_milli,
    autd3_cpu_config_get_ptp_ki_milli
);
cpu_config_bounded_field!(
    [ptp.max_freq_ppb],
    u32,
    MaxFreqPpb,
    autd3_cpu_config_set_ptp_max_freq_ppb,
    autd3_cpu_config_get_ptp_max_freq_ppb
);
cpu_config_bounded_field!(
    [ptp.delay_req_syncs],
    u16,
    NonZeroU16,
    autd3_cpu_config_set_ptp_delay_req_syncs,
    autd3_cpu_config_get_ptp_delay_req_syncs
);
cpu_config_bounded_field!(
    [ptp.path_delay_filter_shift],
    u8,
    PathDelayFilterShift,
    autd3_cpu_config_set_ptp_path_delay_filter_shift,
    autd3_cpu_config_get_ptp_path_delay_filter_shift
);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_cpu_config_set_ptp_pause_quanta(
    handle: *mut CpuConfigHandle,
    quanta: u16,
) -> i32 {
    let Some(config) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    config.0.ptp.pause_quanta = NonZeroU16::new(quanta);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_cpu_config_get_ptp_pause_quanta(
    handle: *const CpuConfigHandle,
    out: *mut u16,
) -> i32 {
    let Some(config) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { autd3_ffi_abi::write_out(out, config.0.ptp.pause_quanta.map_or(0, NonZeroU16::get)) }
}

autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.pause_hold_syncs],
    u16,
    autd3_cpu_config_set_ptp_pause_hold_syncs,
    autd3_cpu_config_get_ptp_pause_hold_syncs
);
autd3_ffi_abi::option_handle_field!(
    CpuConfigHandle,
    [ptp.pause_retry],
    duration,
    autd3_cpu_config_set_ptp_pause_retry,
    autd3_cpu_config_get_ptp_pause_retry
);
autd3_ffi_abi::option_handle_lifecycle!(CpuConfigHandle, autd3_cpu_config_free);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_cpu_config(config: *const CpuConfigHandle) -> *mut Pending {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::SetCpuConfig(Box::new(config.0)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_silencer_completion_time(
    intensity_ns: u64,
    phase_ns: u64,
    strict: bool,
) -> *mut Pending {
    into_handle(Pending::SetSilencerCompletion {
        intensity: Duration::from_nanos(intensity_ns),
        phase: Duration::from_nanos(phase_ns),
        strict,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_silencer_update_rate(intensity: u16, phase: u16) -> *mut Pending {
    let (Some(intensity), Some(phase)) = (NonZeroU16::new(intensity), NonZeroU16::new(phase))
    else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::SetSilencerUpdateRate { intensity, phase })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_silencer_disable() -> *mut Pending {
    into_handle(Pending::SetSilencerDisable)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_gpio_out(outputs: *const Autd3GpioOut) -> *mut Pending {
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
    into_handle(Pending::SetGpioOut([o0, o1, o2, o3]))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_gpio_out_default() -> *mut Pending {
    into_handle(Pending::SetGpioOut(SetGpioOut::default().outputs))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_emulate_gpio_in(values: *const u8) -> *mut Pending {
    let Some(values) = (unsafe { slice_ref(values, 4) }) else {
        return std::ptr::null_mut();
    };

    into_handle(Pending::EmulateGpioIn([
        values[0] != 0,
        values[1] != 0,
        values[2] != 0,
        values[3] != 0,
    ]))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_emulate_gpio_in_default() -> *mut Pending {
    into_handle(Pending::EmulateGpioIn(EmulateGpioIn::default().values))
}

fn total_len(lens: &[usize]) -> Option<usize> {
    lens.iter()
        .try_fold(0usize, |acc, &len| acc.checked_add(len))
}

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_output_mask(
    masks: *const u8,
    lens: *const usize,
    num_devices: usize,
) -> *mut Pending {
    match unsafe { per_device(masks, lens, num_devices, |v| v != 0) } {
        Some(masks) => into_handle(Pending::SetOutputMask(masks)),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_phase_correction(
    phases: *const u8,
    lens: *const usize,
    num_devices: usize,
) -> *mut Pending {
    match unsafe { per_device(phases, lens, num_devices, Phase) } {
        Some(phases) => into_handle(Pending::SetPhaseCorrection(phases)),
        None => std::ptr::null_mut(),
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Autd3PulseWidth {
    pub kind: u8,
    pub raw: u16,
    pub duty: f32,
}

pub const AUTD3_PULSE_WIDTH_RAW: u8 = 0;
pub const AUTD3_PULSE_WIDTH_DUTY: u8 = 1;

fn to_pulse_width(value: Autd3PulseWidth) -> Option<PulseWidth> {
    match value.kind {
        AUTD3_PULSE_WIDTH_RAW => Some(PulseWidth::new(value.raw)),
        AUTD3_PULSE_WIDTH_DUTY => Some(PulseWidth::from_duty(value.duty)),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_pulse_width_table(
    table: *const Autd3PulseWidth,
) -> *mut Pending {
    let Some(slice) = (unsafe { slice_ref(table, PWE_TABLE_SIZE) }) else {
        return std::ptr::null_mut();
    };

    let mut t = Box::new(SetPulseWidthTable::empty_table());
    for (dst, &src) in t.iter_mut().zip(slice.iter()) {
        let Some(value) = to_pulse_width(src) else {
            return std::ptr::null_mut();
        };
        *dst = value;
    }
    into_handle(Pending::SetPulseWidthTable(t))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_set_pulse_width_table_default() -> *mut Pending {
    into_handle(Pending::SetPulseWidthTable(Box::new(
        *SetPulseWidthTable::default().table,
    )))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_resolve(
    pulse_width: Autd3PulseWidth,
    out: *mut u16,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(pulse_width) = to_pulse_width(pulse_width) else {
        unsafe { write_cstr(out_err, out_err_len, "unknown pulse width kind") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    match pulse_width.pulse_width() {
        Ok(value) => unsafe { write_out(out, value) },
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            AUTD3_ERR_INVALID_ARGUMENT
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_foci_stm(
    config: *const StmConfig,
    points: *const Autd3StmControlPoint,
    num_samples: usize,
    num_foci: u8,
    intensities: *const u8,
    bank: u8,
    sound_speed_m_s: f32,
    loop_rep: u16,
    transition_mode: u8,
    transition_value: u64,
) -> *mut Pending {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        to_transition_mode(transition_mode, transition_value),
    ) else {
        return std::ptr::null_mut();
    };

    let Some(points) = (unsafe { foci_points(points, num_samples, num_foci, intensities) }) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::FociStm {
        config: *config,
        points,
        bank,
        sound_speed: sound_speed_m_s,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_pattern_stm(
    config: *const StmConfig,
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
) -> *mut Pending {
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(phase_depth), Some(transition_mode)) = (
        to_pattern_bank(bank),
        PhaseDepth::from_u8(phase_depth),
        to_transition_mode(transition_mode, transition_value),
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
    into_handle(Pending::PatternStm {
        config: *config,
        phases,
        intensities,
        bank,
        phase_depth,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_free(op: *mut Pending) {
    unsafe { drop_handle(op) }
}

impl Pending {
    fn check_each_lengths(&self, num_devices: usize) -> Result<(), String> {
        match self {
            Pending::Each(devices) if devices.len() != num_devices => Err(format!(
                "per-device command holds {} entries for {num_devices} devices",
                devices.len()
            )),
            Pending::Each(devices) => devices
                .iter()
                .flatten()
                .try_for_each(|op| op.check_each_lengths(num_devices)),
            Pending::Sequence(ops) => ops
                .iter()
                .try_for_each(|op| op.check_each_lengths(num_devices)),
            _ => Ok(()),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn boxed(&self) -> BoxedCommand<'_> {
        match self {
            Pending::Each(devices) => each(|device| {
                devices
                    .get(device.idx())
                    .and_then(Option::as_ref)
                    .map(Pending::boxed)
            })
            .boxed(),
            Pending::Sequence(ops) => Sequence(ops).boxed(),
            Pending::Pattern {
                phases,
                intensities,
                bank,
                transition_mode,
            } => Pattern {
                transition_mode: *transition_mode,
                ..Pattern::with_bank(*bank, phases, intensities.as_ref())
            }
            .boxed(),
            Pending::Modulation {
                config,
                data,
                bank,
                loop_behavior,
                transition_mode,
            } => Modulation {
                bank: *bank,
                config: *config,
                data,
                loop_behavior: *loop_behavior,
                transition_mode: *transition_mode,
            }
            .boxed(),
            Pending::WritePatternBuffer {
                bank,
                index,
                phases,
                intensities,
            } => WritePatternBuffer::new(*bank, usize::from(*index), phases, intensities.as_ref())
                .boxed(),
            Pending::ConfigPattern {
                bank,
                config,
                size,
                loop_behavior,
            } => ConfigPattern {
                bank: *bank,
                config: *config,
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                loop_behavior: *loop_behavior,
            }
            .boxed(),
            Pending::ConfigFociStm {
                bank,
                config,
                size,
                num_foci,
                sound_speed,
                loop_behavior,
            } => ConfigFociStm {
                bank: *bank,
                config: *config,
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                num_foci: *num_foci,
                sound_speed: *sound_speed,
                loop_behavior: *loop_behavior,
            }
            .boxed(),
            Pending::ActivatePatternBank {
                bank,
                transition_mode,
            } => ActivatePatternBank {
                bank: *bank,
                transition_mode: *transition_mode,
            }
            .boxed(),
            Pending::WriteModulationBuffer { bank, offset, data } => WriteModulationBuffer {
                bank: *bank,
                offset: usize::try_from(*offset).unwrap_or(usize::MAX),
                data,
            }
            .boxed(),
            Pending::ConfigModulation {
                bank,
                config,
                size,
                loop_behavior,
            } => ConfigModulation {
                bank: *bank,
                config: *config,
                size: usize::try_from(*size).unwrap_or(usize::MAX),
                loop_behavior: *loop_behavior,
            }
            .boxed(),
            Pending::ActivateModulationBank {
                bank,
                transition_mode,
            } => ActivateModulationBank {
                bank: *bank,
                transition_mode: *transition_mode,
            }
            .boxed(),
            Pending::Clear => Clear.boxed(),
            Pending::Synchronize => Synchronize.boxed(),
            Pending::ReleaseFailsafe => ReleaseFailsafe.boxed(),
            Pending::Nop => Nop.boxed(),
            Pending::ForceFan(value) => ForceFan { value: *value }.boxed(),
            Pending::SetCpuConfig(config) => SetCpuConfig::new(**config).boxed(),
            Pending::SetSilencerCompletion {
                intensity,
                phase,
                strict,
            } => SetSilencer::new(FixedCompletionTime {
                intensity: *intensity,
                phase: *phase,
                strict_mode: *strict,
            })
            .boxed(),
            Pending::SetSilencerUpdateRate { intensity, phase } => {
                SetSilencer::new(FixedUpdateRate {
                    intensity: *intensity,
                    phase: *phase,
                })
                .boxed()
            }
            Pending::SetSilencerDisable => SetSilencer::disable().boxed(),
            Pending::SetGpioOut(outputs) => SetGpioOut { outputs: *outputs }.boxed(),
            Pending::EmulateGpioIn(values) => EmulateGpioIn { values: *values }.boxed(),
            Pending::SetOutputMask(masks) => SetOutputMask { masks }.boxed(),
            Pending::SetPhaseCorrection(phases) => SetPhaseCorrection { phases }.boxed(),
            Pending::SetPulseWidthTable(t) => SetPulseWidthTable { table: t }.boxed(),
            Pending::WriteFociBuffer {
                bank,
                index_offset,
                points,
            } => points.boxed_write_foci(*bank, *index_offset),
            Pending::WritePatternPhase {
                bank,
                index,
                depth,
                intensity,
                patterns,
            } => WritePatternPhase {
                bank: *bank,
                index: usize::from(*index),
                depth: *depth,
                intensity: *intensity,
                patterns,
            }
            .boxed(),
            Pending::FociStm {
                config,
                points,
                bank,
                sound_speed,
                loop_behavior,
                transition_mode,
            } => points.boxed_stm(
                *config,
                FociStmOption {
                    bank: *bank,
                    sound_speed: Velocity::from_m_s(*sound_speed),
                    loop_behavior: *loop_behavior,
                    transition_mode: *transition_mode,
                },
            ),
            Pending::PatternStm {
                config,
                phases,
                intensities,
                bank,
                phase_depth,
                loop_behavior,
                transition_mode,
            } => PatternStm::new(
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
        }
    }
}

struct Sequence<'a>(&'a [Pending]);

impl<'a> Command<'a> for Sequence<'a> {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), autd3_rs::Error> {
        for op in self.0 {
            expansion.push(op.boxed())?;
        }
        Ok(())
    }
}

fn is_distinct(ops: &[*mut Pending]) -> bool {
    ops.iter()
        .enumerate()
        .all(|(i, p)| p.is_null() || !ops[..i].contains(p))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_command_each(
    ops: *const *mut Pending,
    num_devices: usize,
) -> *mut Pending {
    let Some(ops) = (unsafe { slice_ref(ops, num_devices) }) else {
        return std::ptr::null_mut();
    };
    if !is_distinct(ops) {
        return std::ptr::null_mut();
    }

    into_handle(Pending::Each(
        ops.iter().map(|&p| unsafe { take_handle(p) }).collect(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_command_sequence(
    ops: *const *mut Pending,
    len: usize,
) -> *mut Pending {
    let Some(ops) = (unsafe { slice_ref(ops, len) }) else {
        return std::ptr::null_mut();
    };
    if ops.iter().any(|p| p.is_null()) || !is_distinct(ops) {
        return std::ptr::null_mut();
    }

    into_handle(Pending::Sequence(
        ops.iter()
            .filter_map(|&p| unsafe { take_handle(p) })
            .collect(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_frames_encode(
    geometry: *const Geometry,
    command: *mut Pending,
    out_code: *mut i32,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut Arc<Frames> {
    let mut frames = Frames::default();
    let code = unsafe { encode_into(&mut frames, geometry, command, out_err, out_err_len) };
    if code == AUTD3_OK {
        into_handle(Arc::new(frames))
    } else {
        unsafe { write_out(out_code, code) };
        std::ptr::null_mut()
    }
}

unsafe fn encode_into(
    frames: &mut Frames,
    geometry: *const Geometry,
    command: *mut Pending,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null geometry") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(pending) = (unsafe { handle_ref(command.cast_const()) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null command") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    if let Err(message) = pending.check_each_lengths(geometry.num_devices()) {
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
pub extern "C" fn autd3_frames_new() -> *mut Arc<Frames> {
    into_handle(Arc::new(Frames::default()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_frames_encode_into(
    frames: *mut Arc<Frames>,
    geometry: *const Geometry,
    command: *mut Pending,
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
    unsafe { encode_into(frames, geometry, command, out_err, out_err_len) }
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

pub struct CheckerHandle(pub(crate) StateChecker);

pub struct ResponseToken(Vec<ResponseFuture>);

pub struct StreamToken(StreamFuture);

pub struct ByteArray(Vec<u8>);

pub struct ResponseHandle(Response);

pub struct BusStatsHandle(BusStats);

pub struct FirmwareVersionArray(Vec<Autd3FirmwareVersion>);

pub struct TelemetryArray(Vec<Autd3Telemetry>);

pub struct DeviceStatus {
    devices: Vec<DeviceState>,
}

pub(crate) fn error_code(e: &Error) -> i32 {
    match e {
        Error::Timeout { .. } | Error::SeqMismatch { .. } => AUTD3_ERR_TIMEOUT,
        Error::DeviceError { .. } | Error::UnexpectedReply { .. } => AUTD3_ERR_DEVICE,
        Error::Network(_) | Error::DeviceLost { .. } => AUTD3_ERR_NETWORK,
        Error::UnsupportedFirmware { .. } => AUTD3_ERR_UNSUPPORTED_FIRMWARE,
        Error::InvalidPayload(_) | Error::Encode(_) => AUTD3_ERR_INVALID_ARGUMENT,
        _ => AUTD3_ERR,
    }
}

fn spawn_completion<T>(
    ctx: CompletionCtx,
    future: impl Future<Output = Result<T, Error>> + Send + 'static,
    into_value: impl FnOnce(T) -> *mut c_void + Send + 'static,
) {
    executor().spawn(async move {
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

fn frame_span(frames: &Frames, frame: i64) -> (usize, usize) {
    match usize::try_from(frame) {
        Ok(index) => (index, 1),
        Err(_) => (0, frames.len()),
    }
}

fn frame_at(frames: &Frames, index: usize) -> Result<autd3_rs::Frame<'_>, Error> {
    frames.frame(index).ok_or_else(|| {
        Error::Network(NetworkCause::new(std::io::Error::other(format!(
            "frame {index} out of range"
        ))))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_open(
    geometry: *const Geometry,
    option: *mut udp::UdpOptionHandle,
    config: *const ClientConfig,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let option = unsafe { take_handle(option) };
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(udp::UdpOptionHandle(option)) = option else {
        ctx.invalid_argument("null argument");
        return;
    };
    unsafe { open_client(geometry, Link::Udp(option), config, ctx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_open_server(
    geometry: *const Geometry,
    link: *mut udp::ServerLinkHandle,
    config: *const ClientConfig,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let link = unsafe { take_handle(link) };
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(udp::ServerLinkHandle(link)) = link else {
        ctx.invalid_argument("null argument");
        return;
    };
    unsafe { open_client(geometry, Link::Server(link), config, ctx) };
}

unsafe fn open_client(
    geometry: *const Geometry,
    link: Link,
    config: *const ClientConfig,
    ctx: CompletionCtx,
) {
    let (Some(geometry), Some(config)) = (unsafe { handle_ref(geometry) }, unsafe {
        handle_ref::<ClientConfig>(config)
    }) else {
        ctx.invalid_argument("null argument");
        return;
    };
    let num_devices = geometry.num_devices();
    if num_devices == 0 || num_devices > autd3_rs::MAX_DEVICES {
        ctx.invalid_argument(&format!(
            "the device count {num_devices} is outside 1..={}",
            autd3_rs::MAX_DEVICES
        ));
        return;
    }

    let geometry = geometry.clone();
    let config = *config;
    spawn_completion(
        ctx,
        async move { Client::open(&geometry, link, config).await },
        |client| into_handle(ClientHandle(Arc::new(client))).cast(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_state_checker(
    client: *const ClientHandle,
) -> *mut CheckerHandle {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return std::ptr::null_mut();
    };
    into_handle(CheckerHandle(client.0.state_checker()))
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
pub unsafe extern "C" fn autd3_client_send_checked(
    client: *const ClientHandle,
    frames: *const Arc<Frames>,
    frame: i64,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, frames, ctx)) = (unsafe { send_ctx(client, frames, cb, user_data) }) else {
        return;
    };
    spawn_completion(
        ctx,
        async move {
            let (start, count) = frame_span(&frames, frame);
            for index in (start..).take(count) {
                client
                    .send_frame(frame_at(&frames, index)?)
                    .await?
                    .await?
                    .check()?;
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
    frame: i64,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, frames, ctx)) = (unsafe { send_ctx(client, frames, cb, user_data) }) else {
        return;
    };
    spawn_completion(
        ctx,
        async move {
            let (start, count) = frame_span(&frames, frame);
            let mut futures = Vec::new();
            for index in (start..).take(count) {
                futures.push(client.send_frame(frame_at(&frames, index)?).await?);
            }
            Ok(futures)
        },
        |futures| into_handle(ResponseToken(futures)).cast(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send_streaming(
    client: *const ClientHandle,
    command: *mut Pending,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, ctx)) = (unsafe { client_ctx(client, cb, user_data) }) else {
        return;
    };
    let Some(pending) = (unsafe { handle_ref(command.cast_const()) }) else {
        ctx.invalid_argument("null command");
        return;
    };
    if let Err(message) = pending.check_each_lengths(client.num_devices()) {
        ctx.invalid_argument(&message);
        return;
    }

    let command = ManuallyDrop::new(unsafe { Box::from_raw(command) });
    executor().spawn(async move {
        let queued = {
            let queued = client.send_streaming(command.boxed());
            queued.await
        };
        match queued {
            Ok(stream) => {
                drop(ManuallyDrop::into_inner(command));
                ctx.ok(into_handle(StreamToken(stream)).cast());
            }
            Err(e) => ctx.fail(error_code(&e), &e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_stream_token_await(
    token: *mut StreamToken,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let token = unsafe { take_handle(token) };
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(StreamToken(stream)) = token else {
        ctx.err("null token");
        return;
    };
    spawn_completion(ctx, stream, |()| std::ptr::null_mut());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_stream_token_free(token: *mut StreamToken) {
    unsafe { drop_handle(token) }
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
    let Some(ResponseToken(futures)) = token else {
        ctx.err("null token");
        return;
    };
    spawn_completion(
        ctx,
        async move {
            let mut merged: Option<Response> = None;
            for future in futures {
                let response = future.await?;
                match merged.as_mut() {
                    None => merged = Some(response),
                    Some(m) => m.merge(&response),
                }
            }
            Ok(merged.unwrap_or_default())
        },
        |response| into_handle(ResponseHandle(response)).cast(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_num_devices(response: *const ResponseHandle) -> usize {
    unsafe { handle_ref(response) }.map_or(0, |r| r.0.status().len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_status(response: *const ResponseHandle) -> *const u8 {
    unsafe { handle_ref(response) }.map_or(std::ptr::null(), |r| r.0.status().as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_value_len(
    response: *const ResponseHandle,
    device: usize,
) -> usize {
    unsafe { handle_ref(response) }.map_or(0, |r| r.0.value(device).len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_value_data(
    response: *const ResponseHandle,
    device: usize,
) -> *const u8 {
    unsafe { handle_ref(response) }.map_or(std::ptr::null(), |r| r.0.value(device).as_ptr())
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
        Some(data) if !data.is_empty() => Response::from_status(data),
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
            let versions = versions.into_iter().map(to_firmware_version).collect();
            into_handle(FirmwareVersionArray(versions)).cast()
        },
    );
}

fn to_firmware_version(version: FirmwareVersion) -> Autd3FirmwareVersion {
    Autd3FirmwareVersion {
        cpu: [version.cpu.major, version.cpu.minor, version.cpu.patch],
        fpga: [version.fpga.major, version.fpga.minor, version.fpga.patch],
        is_emulator: version.is_emulator(),
        is_supported: version.is_supported(),
    }
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
pub unsafe extern "C" fn autd3_client_bus_stats(
    client: *const ClientHandle,
) -> *mut BusStatsHandle {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return std::ptr::null_mut();
    };
    into_handle(BusStatsHandle(client.0.bus_stats()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_frames(stats: *const BusStatsHandle) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.frames())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_resets(stats: *const BusStatsHandle) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.resets())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_heartbeats(stats: *const BusStatsHandle) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.heartbeats())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_missed_heartbeat_replies(
    stats: *const BusStatsHandle,
) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.missed_heartbeat_replies())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_recoveries(stats: *const BusStatsHandle) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.recoveries())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_acked_frames(stats: *const BusStatsHandle) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.acked_frames())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_worst_ack_latency_ns(stats: *const BusStatsHandle) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.worst_ack_latency_ns())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_mean_ack_latency_ns(stats: *const BusStatsHandle) -> u64 {
    unsafe { handle_ref(stats) }.map_or(0, |stats| stats.0.mean_ack_latency_ns())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_bus_stats_free(stats: *mut BusStatsHandle) {
    unsafe { drop_handle(stats) }
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
        |states| into_handle(ByteArray(states.into_iter().map(FpgaState::raw).collect())).cast(),
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
pub unsafe extern "C" fn autd3_checker_check(
    checker: *const CheckerHandle,
    out_code: *mut i32,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut DeviceStatus {
    let Some(checker) = (unsafe { handle_ref(checker) }) else {
        unsafe { write_out(out_code, AUTD3_ERR_INVALID_ARGUMENT) };
        unsafe { write_cstr(out_err, out_err_len, "null checker") };
        return std::ptr::null_mut();
    };

    match checker.0.check().map_err(Error::from) {
        Ok(status) => into_handle(DeviceStatus {
            devices: status.devices().to_vec(),
        }),
        Err(e) => {
            unsafe { write_out(out_code, error_code(&e)) };
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_checker_free(checker: *mut CheckerHandle) {
    unsafe { drop_handle(checker) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_silent_stop(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, ctx)) = (unsafe { client_ctx(client, cb, user_data) }) else {
        return;
    };
    spawn_completion(ctx, async move { client.silent_stop().await }, |()| {
        std::ptr::null_mut()
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_close(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some((client, ctx)) = (unsafe { client_ctx(client, cb, user_data) }) else {
        return;
    };
    spawn_completion(ctx, async move { client.close().await }, |()| {
        std::ptr::null_mut()
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_free(client: *mut ClientHandle) {
    unsafe { drop_handle(client) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_device_status_num_devices(status: *const DeviceStatus) -> usize {
    let Some(status) = (unsafe { handle_ref(status) }) else {
        return 0;
    };

    status.devices.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_device_status_device_state(
    status: *const DeviceStatus,
    index: usize,
    out_kind: *mut u8,
) -> bool {
    let Some(status) = (unsafe { handle_ref(status) }) else {
        return false;
    };

    let Some(state) = status.devices.get(index) else {
        return false;
    };
    unsafe { write_out(out_kind, device_state_code(*state)) == AUTD3_OK }
}

fn device_state_code(state: DeviceState) -> u8 {
    match state {
        DeviceState::Ready => 0,
        DeviceState::Syncing => 1,
        _ => 2,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_device_status_free(status: *mut DeviceStatus) {
    unsafe { drop_handle(status) }
}

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_client_config_matches_the_rust_default() {
        let handle = autd3_client_config_new();
        let config = unsafe { take_handle(handle) }.unwrap();
        let expected = ClientConfig::default();

        assert_eq!(expected.ack_timeout, config.ack_timeout);
        assert_eq!(expected.max_inflight, config.max_inflight);
        assert_eq!(expected.max_resync_rounds, config.max_resync_rounds);
        assert_eq!(
            expected.require_supported_firmware,
            config.require_supported_firmware
        );
        assert_eq!(
            expected.resynchronize_on_recovery,
            config.resynchronize_on_recovery
        );
        assert_eq!(
            expected.release_failsafe_on_recovery,
            config.release_failsafe_on_recovery
        );
    }

    #[test]
    fn a_zero_nonzero_setter_argument_is_rejected() {
        let handle = autd3_client_config_new();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_config_set_max_inflight(handle, 0)
        });
        unsafe { autd3_client_config_free(handle) };
    }

    #[test]
    fn a_zero_ack_timeout_is_left_to_the_open_time_validation() {
        let handle = autd3_client_config_new();
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_set_ack_timeout_ns(handle, 0)
        });
        let config = unsafe { take_handle(handle) }.unwrap();
        assert_eq!(Duration::ZERO, config.ack_timeout);
    }

    fn pending_error(pending: *mut Pending) -> (i32, String) {
        let geometry = Geometry::new(vec![autd3_rs::geometry::Autd3::default()]);
        let mut err = [0 as c_char; 256];
        let mut code = AUTD3_OK;
        let frames = unsafe {
            autd3_frames_encode(
                &raw const geometry,
                pending,
                &raw mut code,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if !frames.is_null() {
            unsafe { autd3_frames_free(frames) };
            return (AUTD3_OK, String::new());
        }
        unsafe { autd3_op_free(pending) };
        let message = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (code, message)
    }

    #[test]
    fn an_unresolvable_modulation_sampling_config_fails_at_encode_with_the_rust_message() {
        let config = SamplingConfig::new(39_999. * Hz);
        let buffer = autd3_ffi_abi::ModulationBuffer(vec![0xFF; 10]);
        let pending = unsafe {
            autd3_op_modulation(0, &raw const config, &raw const buffer, 0xFFFF, 0xFF, 0)
        };
        assert!(!pending.is_null());
        let (code, message) = pending_error(pending);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert!(
            message.contains("must divide the ultrasound frequency"),
            "{message}"
        );
    }

    #[test]
    fn a_per_index_stm_intensity_of_one_buffer_is_not_treated_as_shared() {
        let geometry = Geometry::new(vec![autd3_rs::geometry::Autd3::default()]);
        let phases = autd3_ffi_abi::Buffer(geometry.phase_buffer());
        let intensities = autd3_ffi_abi::Buffer(geometry.intensity_buffer());
        let config = autd3_stm_config_freq(1.0);
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
        assert_eq!(AUTD3_OK, encode(AUTD3_STM_INTENSITY_SHARED).0);
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
        unsafe { autd3_stm_config_free(config) };
    }

    #[test]
    fn a_pulse_width_resolves_with_the_rust_message() {
        let mut out = 0u16;
        let mut err = [0 as c_char; 256];
        let raw = |raw| Autd3PulseWidth {
            kind: AUTD3_PULSE_WIDTH_RAW,
            raw,
            duty: 0.0,
        };
        assert_eq!(AUTD3_OK, unsafe {
            autd3_pulse_width_resolve(raw(256), &raw mut out, err.as_mut_ptr(), err.len())
        });
        assert_eq!(256, out);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_pulse_width_resolve(raw(512), &raw mut out, err.as_mut_ptr(), err.len())
        });
        let message = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }.to_string_lossy();
        assert_eq!(
            message,
            PulseWidth::new(512).pulse_width().unwrap_err().to_string()
        );
        let duty = Autd3PulseWidth {
            kind: AUTD3_PULSE_WIDTH_DUTY,
            raw: 0,
            duty: 0.5,
        };
        assert_eq!(AUTD3_OK, unsafe {
            autd3_pulse_width_resolve(duty, &raw mut out, err.as_mut_ptr(), err.len())
        });
        assert_eq!(256, out);
        let unknown = Autd3PulseWidth {
            kind: 2,
            raw: 0,
            duty: 0.0,
        };
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_pulse_width_resolve(unknown, &raw mut out, err.as_mut_ptr(), err.len())
        });
    }

    #[test]
    fn an_out_of_range_pulse_width_in_the_table_fails_at_encode() {
        let mut table = [Autd3PulseWidth {
            kind: AUTD3_PULSE_WIDTH_RAW,
            raw: 0,
            duty: 0.0,
        }; PWE_TABLE_SIZE];
        table[3].raw = 512;
        let pending = unsafe { autd3_op_set_pulse_width_table(table.as_ptr()) };
        assert!(!pending.is_null());
        let (code, message) = pending_error(pending);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert!(
            message.contains(&PulseWidth::new(512).pulse_width().unwrap_err().to_string()),
            "{message}"
        );
    }

    #[test]
    fn the_client_constants_match_the_rust_constants() {
        assert_eq!(autd3_rs::SERVER_PORT, autd3_params_server_port());
        assert_eq!(autd3_rs::SIMULATOR_PORT, autd3_params_simulator_port());
        assert_eq!(
            FociStmOption::default().sound_speed.m_s(),
            autd3_foci_stm_option_default_sound_speed()
        );
    }

    #[test]
    fn a_new_cpu_config_matches_the_rust_default() {
        let handle = autd3_cpu_config_new();
        let mut ns = 0u64;
        assert_eq!(AUTD3_OK, unsafe {
            autd3_cpu_config_get_sys_time_transition_margin(handle, &raw mut ns)
        });
        assert_eq!(
            CpuConfig::default().sys_time_transition_margin,
            Duration::from_nanos(ns)
        );
        let config = unsafe { take_handle(handle) }.unwrap();
        assert_eq!(CpuConfig::default(), config.0);
    }

    #[test]
    fn cpu_config_setters_reach_their_fields() {
        let handle = autd3_cpu_config_new();
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_sys_time_transition_margin(handle, 0)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_fpga_wait_update_max_polls(handle, 11)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_fpga_flash_max_polls(handle, 12)
            );
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_sync_guard(handle, 13));
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_update_activate_delay(handle, 14_000_000)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_failsafe_timeout(handle, true, 25_000_000)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_unlock_failsafe_timeout(handle, true, 26_000_000)
            );
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_fpga_bus_wait(handle, 2));
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_cpu_watchdog_timeout(handle, true, 27_000_000)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_sync_interval(handle, 15_000_000)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_tx_timestamp_timeout(handle, 16_000_000)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_delay_resp_timeout(handle, 17_000_000)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_holdover(handle, 18_000_000)
            );
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_ptp_lock_samples(handle, 19));
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_step_threshold(handle, 20)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_lock_threshold(handle, 21)
            );
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_ptp_kp_milli(handle, 22));
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_ptp_ki_milli(handle, 23));
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_ptp_max_freq_ppb(handle, 24));
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_ptp_max_freq_ppb(handle, 0x8000_0000)
            );
        }
        let op = unsafe { autd3_op_set_cpu_config(handle) };
        assert!(!op.is_null());
        unsafe { autd3_op_free(op) };

        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(
            config,
            CpuConfig {
                sys_time_transition_margin: Duration::ZERO,
                fpga_wait_update_max_polls: NonZeroU32::new(11).unwrap(),
                fpga_flash_max_polls: NonZeroU32::new(12).unwrap(),
                sync_guard: Duration::from_nanos(13),
                update_activate_delay: Duration::from_millis(14),
                failsafe_timeout: Some(Duration::from_millis(25)),
                ptp_unlock_failsafe_timeout: Some(Duration::from_millis(26)),
                fpga_bus_wait: autd3_rs::commands::FpgaBusWait::Cycles2,
                cpu_watchdog_timeout: Some(Duration::from_millis(27)),
                ptp: autd3_rs::commands::PtpConfig {
                    sync_interval: Duration::from_millis(15),
                    tx_timestamp_timeout: Duration::from_millis(16),
                    delay_resp_timeout: Duration::from_millis(17),
                    holdover: Duration::from_millis(18),
                    lock_samples: NonZeroU16::new(19).unwrap(),
                    step_threshold: Duration::from_nanos(20),
                    lock_threshold: Duration::from_nanos(21),
                    kp_milli: 22,
                    ki_milli: 23,
                    max_freq_ppb: autd3_rs::commands::MaxFreqPpb::new(24).unwrap(),
                    ..autd3_rs::commands::PtpConfig::default()
                },
            }
        );
    }

    #[test]
    fn the_ptp_delay_request_and_pause_fields_reach_the_config() {
        let handle = autd3_cpu_config_new();
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_delay_req_syncs(handle, 26)
            );
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_ptp_delay_req_syncs(handle, 0)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_path_delay_filter_shift(handle, 4)
            );
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_ptp_path_delay_filter_shift(handle, 17)
            );
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_ptp_pause_quanta(handle, 27));
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_pause_hold_syncs(handle, 28)
            );
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_pause_retry(handle, 29_000_000)
            );
        }
        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(
            config.ptp,
            autd3_rs::commands::PtpConfig {
                delay_req_syncs: NonZeroU16::new(26).unwrap(),
                path_delay_filter_shift: autd3_rs::commands::PathDelayFilterShift::new(4).unwrap(),
                pause_quanta: NonZeroU16::new(27),
                pause_hold_syncs: 28,
                pause_retry: Duration::from_millis(29),
                ..autd3_rs::commands::PtpConfig::default()
            }
        );
    }

    #[test]
    fn a_zero_pause_quanta_disables_the_pause() {
        let handle = autd3_cpu_config_new();
        let mut quanta = 0u16;
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_get_ptp_pause_quanta(handle, &raw mut quanta)
            );
            assert_eq!(quanta, 48);
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_ptp_pause_quanta(handle, 0));
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_get_ptp_pause_quanta(handle, &raw mut quanta)
            );
            assert_eq!(quanta, 0);
        }
        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(config.ptp.pause_quanta, None);
    }

    fn optional_duration(
        get: unsafe extern "C" fn(*const CpuConfigHandle, *mut bool, *mut u64) -> i32,
        handle: *const CpuConfigHandle,
    ) -> Option<Duration> {
        let mut has_value = false;
        let mut ns = u64::MAX;
        assert_eq!(AUTD3_OK, unsafe {
            get(handle, &raw mut has_value, &raw mut ns)
        });
        has_value.then(|| Duration::from_nanos(ns))
    }

    #[test]
    fn the_failsafe_timeout_presence_is_explicit_and_zero_reaches_the_validation() {
        let handle = autd3_cpu_config_new();
        assert_eq!(
            Some(Duration::from_millis(500)),
            optional_duration(autd3_cpu_config_get_failsafe_timeout, handle)
        );
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_failsafe_timeout(handle, false, 1)
            );
        }
        assert_eq!(
            None,
            optional_duration(autd3_cpu_config_get_failsafe_timeout, handle)
        );
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_failsafe_timeout(handle, true, 0)
            );
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_failsafe_timeout(std::ptr::null_mut(), false, 0)
            );
        }
        assert_eq!(
            Some(Duration::ZERO),
            optional_duration(autd3_cpu_config_get_failsafe_timeout, handle)
        );
        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(config.failsafe_timeout, Some(Duration::ZERO));
    }

    #[test]
    fn the_fpga_bus_wait_is_three_cycles_until_it_is_set_and_rejects_other_counts() {
        let handle = autd3_cpu_config_new();
        let mut cycles = 0u8;
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_get_fpga_bus_wait(handle, &raw mut cycles)
            );
            assert_eq!(cycles, 3);
            for invalid in [0, 1, 4, u8::MAX] {
                assert_eq!(
                    AUTD3_ERR_INVALID_ARGUMENT,
                    autd3_cpu_config_set_fpga_bus_wait(handle, invalid)
                );
            }
            assert_eq!(AUTD3_OK, autd3_cpu_config_set_fpga_bus_wait(handle, 2));
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_get_fpga_bus_wait(handle, &raw mut cycles)
            );
            assert_eq!(cycles, 2);
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_fpga_bus_wait(std::ptr::null_mut(), 2)
            );
        }
        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(
            config.fpga_bus_wait,
            autd3_rs::commands::FpgaBusWait::Cycles2
        );
    }

    #[test]
    fn the_ptp_unlock_failsafe_timeout_is_none_until_it_is_set() {
        let handle = autd3_cpu_config_new();
        assert_eq!(
            None,
            optional_duration(autd3_cpu_config_get_ptp_unlock_failsafe_timeout, handle)
        );
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_ptp_unlock_failsafe_timeout(handle, true, 2_000_000_000)
            );
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_ptp_unlock_failsafe_timeout(std::ptr::null_mut(), true, 0)
            );
        }
        assert_eq!(
            Some(Duration::from_secs(2)),
            optional_duration(autd3_cpu_config_get_ptp_unlock_failsafe_timeout, handle)
        );
        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(
            config.ptp_unlock_failsafe_timeout,
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn the_cpu_watchdog_timeout_is_one_second_until_it_is_cleared() {
        let handle = autd3_cpu_config_new();
        assert_eq!(
            Some(Duration::from_secs(1)),
            optional_duration(autd3_cpu_config_get_cpu_watchdog_timeout, handle)
        );
        unsafe {
            assert_eq!(
                AUTD3_OK,
                autd3_cpu_config_set_cpu_watchdog_timeout(handle, false, 0)
            );
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_cpu_watchdog_timeout(std::ptr::null_mut(), false, 0)
            );
        }
        assert_eq!(
            None,
            optional_duration(autd3_cpu_config_get_cpu_watchdog_timeout, handle)
        );
        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(config.cpu_watchdog_timeout, None);
    }

    #[test]
    fn a_zero_cpu_config_count_is_rejected_and_keeps_the_value() {
        let handle = autd3_cpu_config_new();
        unsafe {
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_fpga_wait_update_max_polls(handle, 0)
            );
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_fpga_flash_max_polls(handle, 0)
            );
            assert_eq!(
                AUTD3_ERR_INVALID_ARGUMENT,
                autd3_cpu_config_set_ptp_lock_samples(handle, 0)
            );
        }
        let config = unsafe { take_handle(handle) }.unwrap().0;
        assert_eq!(CpuConfig::default(), config);
        assert!(unsafe { autd3_op_set_cpu_config(std::ptr::null()) }.is_null());
    }

    #[test]
    fn unknown_enum_discriminants_are_rejected() {
        assert!(to_pattern_bank(2).is_none());
        assert!(to_modulation_bank(2).is_none());
        assert!(to_gpio_in(4).is_none());
        assert!(PhaseDepth::from_u8(0).is_none());
        assert!(PhaseDepth::from_u8(2).is_none());
        assert!(to_transition_mode(0x03, 0).is_none());
        assert!(to_gpio_out(&Autd3GpioOut { kind: 14, value: 0 }).is_none());
    }

    #[test]
    fn telemetry_array_hands_out_every_field_of_every_device() {
        let mut telemetry = Telemetry::default();
        telemetry.processed = 0x1234_5678;
        telemetry.failsafe = 0x0708;
        telemetry.boot_failure = 9;
        telemetry.cpu_watchdog = 10;
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
    fn a_period_crosses_the_boundary_in_nanoseconds() {
        let mut err = [0 as c_char; 256];
        for (period_ns, size, expected) in [
            (100_000_000u64, 4usize, 1000u16),
            (333_000_000, 3, 4440),
            (1_000_000, 1, 40),
        ] {
            let handle = autd3_stm_config_period(period_ns);
            let mut out = 0u16;
            assert_eq!(AUTD3_OK, unsafe {
                autd3_stm_config_into_sampling_config(
                    handle,
                    size,
                    &raw mut out,
                    err.as_mut_ptr(),
                    err.len(),
                )
            });
            assert_eq!(expected, out);
            unsafe { autd3_stm_config_free(handle) };
        }
    }

    #[test]
    fn an_indivisible_period_reports_the_rust_message() {
        let mut err = [0 as c_char; 256];
        let handle = autd3_stm_config_period(100_000_001);
        let mut out = 0u16;
        assert_eq!(AUTD3_ERR, unsafe {
            autd3_stm_config_into_sampling_config(
                handle,
                4,
                &raw mut out,
                err.as_mut_ptr(),
                err.len(),
            )
        });
        let message = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }.to_string_lossy();
        assert!(message.contains("must be divisible"), "{message}");
        unsafe { autd3_stm_config_free(handle) };
    }

    #[test]
    fn the_client_config_getters_read_the_rust_default() {
        let handle = autd3_client_config_new();
        let expected = ClientConfig::default();
        let mut ns = 0u64;
        let mut inflight = 0usize;
        let mut rounds = 0u32;
        let mut require = true;
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_ack_timeout_ns(handle, &raw mut ns)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_max_inflight(handle, &raw mut inflight)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_max_resync_rounds(handle, &raw mut rounds)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_require_supported_firmware(handle, &raw mut require)
        });
        assert_eq!(expected.ack_timeout, Duration::from_nanos(ns));
        assert_eq!(expected.max_inflight.get(), inflight);
        assert_eq!(expected.max_resync_rounds.get(), rounds);
        assert_eq!(expected.require_supported_firmware, require);
        let mut resynchronize = u32::MAX;
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_resynchronize_on_recovery(handle, &raw mut resynchronize)
        });
        assert_eq!(
            expected.resynchronize_on_recovery,
            NonZeroU32::new(resynchronize)
        );
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_set_resynchronize_on_recovery(handle, 0)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_resynchronize_on_recovery(handle, &raw mut resynchronize)
        });
        assert_eq!(resynchronize, 0);
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_set_resynchronize_on_recovery(handle, 5)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_resynchronize_on_recovery(handle, &raw mut resynchronize)
        });
        assert_eq!(resynchronize, 5);
        let mut release_failsafe = u32::MAX;
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_release_failsafe_on_recovery(handle, &raw mut release_failsafe)
        });
        assert_eq!(
            expected.release_failsafe_on_recovery,
            NonZeroU32::new(release_failsafe)
        );
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_set_release_failsafe_on_recovery(handle, 0)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_release_failsafe_on_recovery(handle, &raw mut release_failsafe)
        });
        assert_eq!(release_failsafe, 0);
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_set_release_failsafe_on_recovery(handle, 2)
        });
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_get_release_failsafe_on_recovery(handle, &raw mut release_failsafe)
        });
        assert_eq!(release_failsafe, 2);
        unsafe { autd3_client_config_free(handle) };
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
    fn the_gpio_and_fan_default_ops_match_rust() {
        assert!(matches!(
            unsafe { take_handle(autd3_op_force_fan_default()) },
            Some(Pending::ForceFan(value)) if value == ForceFan::default().value
        ));
        assert!(matches!(
            unsafe { take_handle(autd3_op_set_gpio_out_default()) },
            Some(Pending::SetGpioOut(outputs)) if outputs == SetGpioOut::default().outputs
        ));
        assert!(matches!(
            unsafe { take_handle(autd3_op_emulate_gpio_in_default()) },
            Some(Pending::EmulateGpioIn(values)) if values == EmulateGpioIn::default().values
        ));
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
            assert_eq!(
                state.is_failsafe_active(),
                autd3_fpga_state_is_failsafe_active(raw)
            );
        }
    }

    #[test]
    fn a_size_wider_than_u32_is_rejected_instead_of_dividing_by_zero() {
        let handle = autd3_stm_config_period(1_000_000_000);
        let mut out = 0u16;
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_stm_config_into_sampling_config(
                handle,
                1usize << 32,
                &raw mut out,
                std::ptr::null_mut(),
                0,
            )
        });
        unsafe { autd3_stm_config_free(handle) };
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
        assert!(
            unsafe { autd3_op_set_phase_correction(values.as_ptr(), lens.as_ptr(), lens.len()) }
                .is_null()
        );
    }

    fn geometry_of(num_devices: usize) -> *mut Geometry {
        into_handle(Geometry::new(
            (0..num_devices)
                .map(|_| {
                    autd3_rs::Autd3::new(Point3::origin(), autd3_rs::UnitQuaternion::identity())
                })
                .collect(),
        ))
    }

    fn encode(geometry: *const Geometry, command: *mut Pending) -> *mut Arc<Frames> {
        unsafe {
            autd3_frames_encode(
                geometry,
                command,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        }
    }

    fn num_frames_of(geometry: *const Geometry, command: *mut Pending) -> usize {
        let frames = encode(geometry, command);
        assert!(!frames.is_null());
        let len = unsafe { autd3_frames_num_frames(frames) };
        unsafe { autd3_frames_free(frames) };
        len
    }

    fn two_entries_for_one_device() -> *mut Pending {
        let per_device = [autd3_op_nop(), autd3_op_nop()];
        unsafe { autd3_command_each(per_device.as_ptr(), per_device.len()) }
    }

    #[test]
    fn a_length_mismatch_leaves_the_command_handle_with_the_caller() {
        let one = geometry_of(1);
        let two = geometry_of(2);

        let command = two_entries_for_one_device();
        let mut code = AUTD3_OK;
        assert!(
            unsafe { autd3_frames_encode(one, command, &raw mut code, std::ptr::null_mut(), 0) }
                .is_null()
        );
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        assert_eq!(1, num_frames_of(two, command));

        let command = two_entries_for_one_device();
        let frames = autd3_frames_new();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_frames_encode_into(frames, one, command, std::ptr::null_mut(), 0)
        });
        unsafe { autd3_op_free(command) };

        unsafe { autd3_frames_free(frames) };
        unsafe { drop_handle(one) };
        unsafe { drop_handle(two) };
    }

    #[test]
    fn a_rejected_encode_leaves_the_command_handle_with_the_caller() {
        let geometry = geometry_of(1);
        let op = autd3_op_clear();

        assert!(encode(std::ptr::null(), op).is_null());
        assert!(encode(geometry, std::ptr::null_mut()).is_null());

        assert_eq!(1, num_frames_of(geometry, op));
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_rejected_encode_reports_the_reason() {
        let geometry = geometry_of(1);
        let mut err = [0 as c_char; 64];

        let frames = unsafe {
            autd3_frames_encode(
                geometry,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                err.as_mut_ptr(),
                err.len(),
            )
        };

        assert!(frames.is_null());
        let message = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) };
        assert_eq!("null command", message.to_str().unwrap());
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_command_that_fails_to_encode_stays_with_the_caller_and_yields_no_frames() {
        let empty = geometry_of(0);
        let one = geometry_of(1);
        let mut err = [0 as c_char; 128];

        let mut code = AUTD3_OK;
        let command = autd3_op_clear();

        let frames = unsafe {
            autd3_frames_encode(empty, command, &raw mut code, err.as_mut_ptr(), err.len())
        };

        assert!(frames.is_null());
        assert_ne!(0, err[0]);
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);

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
    fn a_rejected_encode_reports_the_error_code() {
        let geometry = geometry_of(1);
        let mut code = AUTD3_OK;

        let frames = unsafe {
            autd3_frames_encode(
                geometry,
                std::ptr::null_mut(),
                &raw mut code,
                std::ptr::null_mut(),
                0,
            )
        };

        assert!(frames.is_null());
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_successful_encode_leaves_the_error_code_untouched() {
        let geometry = geometry_of(1);
        let mut code = AUTD3_OK;

        let frames = unsafe {
            autd3_frames_encode(
                geometry,
                autd3_op_clear(),
                &raw mut code,
                std::ptr::null_mut(),
                0,
            )
        };

        assert!(!frames.is_null());
        assert_eq!(AUTD3_OK, code);
        unsafe { autd3_frames_free(frames) };
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_null_bus_stats_handle_reads_as_zero() {
        assert!(unsafe { autd3_client_bus_stats(std::ptr::null()) }.is_null());
        assert_eq!(0, unsafe { autd3_bus_stats_frames(std::ptr::null()) });
        assert_eq!(0, unsafe {
            autd3_bus_stats_mean_ack_latency_ns(std::ptr::null())
        });
        unsafe { autd3_bus_stats_free(std::ptr::null_mut()) };
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
    fn a_rejected_sequence_leaves_every_op_handle_with_the_caller() {
        let first = autd3_op_clear();
        let second = autd3_op_nop();

        assert!(unsafe { autd3_command_sequence(std::ptr::null(), 2) }.is_null());

        let with_null = [first, std::ptr::null_mut(), second];
        assert!(unsafe { autd3_command_sequence(with_null.as_ptr(), with_null.len()) }.is_null());

        let aliased = [first, second, first];
        assert!(unsafe { autd3_command_sequence(aliased.as_ptr(), aliased.len()) }.is_null());

        let geometry = geometry_of(1);
        let ops = [first, second];
        let command = unsafe { autd3_command_sequence(ops.as_ptr(), ops.len()) };
        assert!(!command.is_null());
        assert_eq!(2, num_frames_of(geometry, command));
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn an_unsent_composed_command_is_freed_with_its_children() {
        let ops = [autd3_op_clear(), std::ptr::null_mut()];
        let per_device = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };
        let ops = [per_device, autd3_op_nop()];
        let sequence = unsafe { autd3_command_sequence(ops.as_ptr(), ops.len()) };
        assert!(!sequence.is_null());

        unsafe { autd3_op_free(sequence) };
    }

    #[test]
    fn the_device_time_of_a_null_client_is_an_error() {
        let mut ns = 0u64;
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_device_time_now(std::ptr::null(), &raw mut ns, std::ptr::null_mut(), 0)
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
    fn each_spans_the_longest_per_device_command() {
        let geometry = geometry_of(2);
        let long = [autd3_op_clear(), autd3_op_nop(), autd3_op_synchronize()];
        let long = unsafe { autd3_command_sequence(long.as_ptr(), long.len()) };
        let ops = [autd3_op_nop(), long];
        let command = unsafe { autd3_command_each(ops.as_ptr(), ops.len()) };

        assert_eq!(3, num_frames_of(geometry, command));
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_sequence_expands_its_commands_in_order() {
        let geometry = geometry_of(2);
        let per_device = [autd3_op_nop(), std::ptr::null_mut()];
        let per_device = unsafe { autd3_command_each(per_device.as_ptr(), per_device.len()) };
        let inner = [autd3_op_nop(), autd3_op_nop()];
        let inner = unsafe { autd3_command_sequence(inner.as_ptr(), inner.len()) };
        let ops = [autd3_op_clear(), per_device, inner];
        let command = unsafe { autd3_command_sequence(ops.as_ptr(), ops.len()) };

        assert_eq!(4, num_frames_of(geometry, command));
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
        let ops = [autd3_op_clear(), autd3_op_nop()];
        let sequence = unsafe { autd3_command_sequence(ops.as_ptr(), ops.len()) };
        assert_eq!(AUTD3_OK, unsafe {
            autd3_frames_encode_into(frames, geometry, sequence, std::ptr::null_mut(), 0)
        });
        assert_eq!(2, unsafe { autd3_frames_num_frames(frames) });
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

    #[test]
    fn an_empty_sequence_encodes_to_no_frames() {
        let geometry = geometry_of(1);
        let command = unsafe { autd3_command_sequence(std::ptr::null(), 0) };
        assert!(!command.is_null());

        assert_eq!(0, num_frames_of(geometry, command));
        unsafe { drop_handle(geometry) };
    }

    fn encode_error(geometry: *const Geometry, command: *mut Pending) -> String {
        let mut err = [0 as c_char; 128];
        let frames = unsafe {
            autd3_frames_encode(
                geometry,
                command,
                std::ptr::null_mut(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        assert!(frames.is_null());
        unsafe { autd3_op_free(command) };
        unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned()
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
    fn a_nested_each_that_does_not_match_the_device_count_fails_to_encode() {
        let geometry = geometry_of(2);

        let inner = [autd3_op_nop()];
        let inner = unsafe { autd3_command_each(inner.as_ptr(), inner.len()) };
        let outer = [std::ptr::null_mut(), inner];
        let outer = unsafe { autd3_command_each(outer.as_ptr(), outer.len()) };
        assert!(encode_error(geometry, outer).contains("1 entries for 2 devices"));

        let inner = [autd3_op_nop(), autd3_op_nop(), autd3_op_nop()];
        let inner = unsafe { autd3_command_each(inner.as_ptr(), inner.len()) };
        let sequence = [autd3_op_clear(), inner];
        let sequence = unsafe { autd3_command_sequence(sequence.as_ptr(), sequence.len()) };
        assert!(encode_error(geometry, sequence).contains("3 entries for 2 devices"));

        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn the_frame_count_of_a_null_frames_handle_is_zero() {
        assert_eq!(0, unsafe { autd3_frames_num_frames(std::ptr::null()) });
        unsafe { autd3_frames_free(std::ptr::null_mut()) };
    }

    #[test]
    fn a_null_completion_callback_is_ignored() {
        unsafe { autd3_client_close(std::ptr::null(), None, std::ptr::null_mut()) };
    }
}
