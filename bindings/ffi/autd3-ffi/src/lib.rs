use std::ffi::{CString, c_char, c_void};
use std::num::{NonZeroU16, NonZeroU32, NonZeroUsize};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use autd3_ffi_abi::{
    CheckerBackend, ClientBackend, ClientOpener, CompletionCallback, CompletionCtx, DevicePattern,
    ModulationBuffer, PatternBuffer, ResponseTokenData, drop_handle, into_handle,
};
use autd3_rs::commands::{
    BoxedCommand, ChangeModulationBank, ChangePatternBank, Clear, Command, ConfigFociStm,
    ConfigModulation, ConfigPattern, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate,
    FociStm as CoreFociStm, FociStmOption, ForceFan, GpioOut, Modulation, Nop, PWE_TABLE_SIZE,
    Pattern, PatternCompression, PatternStm, PatternStmMode, PatternStmOption, SetGpioOut,
    SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, StmConfig, Synchronize,
    WriteFociBuffer, WriteModulationBuffer, WritePatternBuffer, WritePatternCompressed, circle,
    line,
};
use autd3_rs::units::Hz;
use autd3_rs::value::{
    ControlPoint, ControlPoints, DcSysTime, GpioIn, Intensity, LoopBehavior, ModulationBank,
    Nearest, PatternBank, Phase, PulseWidth, SamplingConfig, TransitionMode,
};
use autd3_rs::{
    ClientConfig, CoreId, DatagramBuilder as CoreDatagramBuilder, Frames, Geometry, Length, Point3,
    Response, RtSchedulePolicy, ThreadPriority, ThreadPriorityValue, UnitVector3, Vector3,
    Velocity,
};
use autd3_rs::{DeviceState, Telemetry};
use tokio::runtime::{Builder, Runtime};

mod legacy;

pub(crate) fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build autd3 ffi runtime")
    })
}

pub(crate) unsafe fn write_cstr(buf: *mut c_char, len: usize, s: &str) {
    if buf.is_null() || len == 0 {
        return;
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(len - 1);

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buf, n);
        *buf.add(n) = 0;
    }
}

fn to_pattern_bank(v: u8) -> PatternBank {
    if v == 1 {
        PatternBank::B1
    } else {
        PatternBank::B0
    }
}

fn to_modulation_bank(v: u8) -> ModulationBank {
    if v == 1 {
        ModulationBank::B1
    } else {
        ModulationBank::B0
    }
}

fn to_gpio_in(v: u8) -> GpioIn {
    match v {
        1 => GpioIn::I1,
        2 => GpioIn::I2,
        3 => GpioIn::I3,
        _ => GpioIn::I0,
    }
}

fn to_telemetry(counter: u8) -> Option<Telemetry> {
    match counter {
        0x00 => Some(Telemetry::FifoDrop),
        0x01 => Some(Telemetry::Dedup),
        0x02 => Some(Telemetry::SeqMismatch),
        0x03 => Some(Telemetry::DispatchError),
        0x04 => Some(Telemetry::Processed),
        0x05 => Some(Telemetry::Failsafe),
        _ => None,
    }
}

fn to_transition_mode(mode: u8, value: u64, margin_ns: u32) -> TransitionMode {
    match mode {
        0x01 => TransitionMode::SysTime {
            time: DcSysTime::from_nanos(value),
            margin: (margin_ns != 0).then(|| Duration::from_nanos(u64::from(margin_ns))),
        },
        #[allow(clippy::cast_possible_truncation)]
        0x02 => TransitionMode::Gpio(to_gpio_in(value as u8)),
        0xF0 => TransitionMode::Ext,
        0xFE => TransitionMode::Later,
        0xFF => TransitionMode::Immediate,
        _ => TransitionMode::SyncIdx,
    }
}

#[repr(C)]
pub struct Autd3GpioOut {
    pub kind: u8,
    pub value: u64,
}

#[allow(clippy::cast_possible_truncation)]
fn to_gpio_out(g: &Autd3GpioOut) -> GpioOut {
    match g.kind {
        1 => GpioOut::BaseSignal,
        2 => GpioOut::Thermo,
        3 => GpioOut::ForceFan,
        4 => GpioOut::Sync,
        5 => GpioOut::ModBank,
        6 => GpioOut::ModIdx(g.value as u16),
        7 => GpioOut::PatternBank,
        8 => GpioOut::PatternIdx(g.value as u16),
        9 => GpioOut::IsStmMode,
        10 => GpioOut::SysTimeEq(DcSysTime::from_nanos(g.value)),
        11 => GpioOut::SyncDiff,
        12 => GpioOut::PwmOut(g.value as u8),
        13 => GpioOut::Direct(g.value != 0),
        _ => GpioOut::Off,
    }
}

fn rep_to_loop_behavior(rep: u16) -> LoopBehavior {
    if rep == 0xFFFF {
        LoopBehavior::Infinite
    } else {
        NonZeroU16::new(rep + 1).map_or(LoopBehavior::Infinite, LoopBehavior::Finite)
    }
}

fn to_pattern_stm_mode(mode: u8) -> PatternStmMode {
    match mode {
        1 => PatternStmMode::PhaseFull,
        2 => PatternStmMode::PhaseHalf,
        _ => PatternStmMode::PhaseIntensityFull,
    }
}

#[repr(C)]
pub struct Autd3StmControlPoint {
    pub point: [f32; 3],
    pub phase_offset: u8,
}

pub struct FociSample {
    intensity: Intensity,
    points: Vec<ControlPoint>,
}

macro_rules! foci_points {
    ($($n:literal => $variant:ident),* $(,)?) => {
        pub enum FociPoints {
            $($variant(Vec<ControlPoints<$n>>)),*
        }

        impl FociPoints {
            fn from_samples(samples: &[FociSample], num_foci: usize) -> Option<Self> {
                match num_foci {
                    $($n => Some(FociPoints::$variant(
                        samples
                            .iter()
                            .map(|s| {
                                let arr: [ControlPoint; $n] = core::array::from_fn(|k| s.points[k]);
                                ControlPoints::new(arr, s.intensity)
                            })
                            .collect(),
                    )),)*
                    _ => None,
                }
            }

            fn push_into<'a>(
                &'a self,
                config: StmConfig,
                option: FociStmOption,
                builder: &mut CoreDatagramBuilder<'a>,
            ) {
                match self {
                    $(FociPoints::$variant(v) => {
                        builder.push(CoreFociStm::new(config, v.as_slice(), option));
                    })*
                }
            }

            pub(crate) fn push_legacy_into<'a>(
                &'a self,
                config: StmConfig,
                option: FociStmOption,
                builder: &mut autd3_rs::legacy::LegacyDatagramBuilder<'a>,
            ) {
                match self {
                    $(FociPoints::$variant(v) => {
                        builder.push(CoreFociStm::new(config, v.as_slice(), option));
                    })*
                }
            }

            fn push_write_foci_into<'a>(
                &'a self,
                bank: PatternBank,
                index_offset: usize,
                builder: &mut CoreDatagramBuilder<'a>,
            ) {
                match self {
                    $(FociPoints::$variant(v) => {
                        builder.push(WriteFociBuffer {
                            bank,
                            index_offset,
                            points: v.as_slice(),
                        });
                    })*
                }
            }
        }
    };
}
foci_points!(1 => N1, 2 => N2, 3 => N3, 4 => N4, 5 => N5, 6 => N6, 7 => N7, 8 => N8);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_freq(hz: f32) -> *mut StmConfig {
    into_handle(StmConfig::new(hz * Hz))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_freq_nearest(hz: f32) -> *mut StmConfig {
    into_handle(StmConfig::new(Nearest(hz * Hz)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_period(secs: f32) -> *mut StmConfig {
    into_handle(StmConfig::new(Duration::from_secs_f32(secs)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_period_nearest(secs: f32) -> *mut StmConfig {
    into_handle(StmConfig::new(Nearest(Duration::from_secs_f32(secs))))
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
) -> i32 {
    if config.is_null() || out.is_null() {
        return -1;
    }

    // SAFETY: the caller guarantees `config` points to a valid StmConfig handle.
    let Ok(value) = unsafe { *config }.into_sampling_config(size).divide() else {
        return -1;
    };

    // SAFETY: the caller guarantees `out` points to a writable u16.
    unsafe { *out = value };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_stm_config_free(config: *mut StmConfig) {
    unsafe { drop_handle(config) }
}

unsafe fn write_control_points(
    points: &[ControlPoints<1>],
    out_points: *mut Autd3StmControlPoint,
    out_intensities: *mut u8,
) {
    for (i, cp) in points.iter().enumerate() {
        let p = cp.points[0];
        unsafe {
            *out_points.add(i) = Autd3StmControlPoint {
                point: [p.point.x, p.point.y, p.point.z],
                phase_offset: p.phase_offset.0,
            };
            *out_intensities.add(i) = cp.intensity.0;
        }
    }
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
    if center.is_null() || normal.is_null() || out_points.is_null() || out_intensities.is_null() {
        return -1;
    }
    let center = unsafe { std::slice::from_raw_parts(center, 3) };
    let normal = unsafe { std::slice::from_raw_parts(normal, 3) };
    let mut points = Vec::new();
    circle(
        Point3::new(center[0], center[1], center[2]),
        Length::millimeters(radius_mm),
        num_points,
        UnitVector3::new_normalize(Vector3::new(normal[0], normal[1], normal[2])),
        Intensity(intensity),
        &mut points,
    );
    unsafe { write_control_points(&points, out_points, out_intensities) };
    0
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
    if start.is_null() || end.is_null() || out_points.is_null() || out_intensities.is_null() {
        return -1;
    }
    let start = unsafe { std::slice::from_raw_parts(start, 3) };
    let end = unsafe { std::slice::from_raw_parts(end, 3) };
    let mut points = Vec::new();
    line(
        Point3::new(start[0], start[1], start[2]),
        Point3::new(end[0], end[1], end[2]),
        num_points,
        Intensity(intensity),
        &mut points,
    );
    unsafe { write_control_points(&points, out_points, out_intensities) };
    0
}

pub const AUTD3_RT_PRIORITY_DEFAULT: u8 = 0;
pub const AUTD3_RT_PRIORITY_DISABLED: u8 = 1;
pub const AUTD3_RT_PRIORITY_EXPLICIT: u8 = 2;

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn autd3_client_config_new(
    low_latency: bool,
    timeout_cycles: u32,
    max_inflight: usize,
    max_resync_rounds: u32,
    reset_resend_cycles: u32,
    rt_priority_mode: u8,
    rt_priority: u8,
    has_rt_affinity: bool,
    rt_affinity: usize,
    validate_state: bool,
) -> *mut ClientConfig {
    let (
        Some(timeout_cycles),
        Some(max_inflight),
        Some(max_resync_rounds),
        Some(reset_resend_cycles),
    ) = (
        NonZeroU32::new(timeout_cycles),
        NonZeroUsize::new(max_inflight),
        NonZeroU32::new(max_resync_rounds),
        NonZeroU32::new(reset_resend_cycles),
    )
    else {
        return std::ptr::null_mut();
    };
    let rt_priority = match rt_priority_mode {
        AUTD3_RT_PRIORITY_DEFAULT => ClientConfig::default().rt_priority,
        AUTD3_RT_PRIORITY_DISABLED => None,
        AUTD3_RT_PRIORITY_EXPLICIT => match ThreadPriorityValue::try_from(rt_priority) {
            Ok(value) => Some(ThreadPriority::Crossplatform(value)),
            Err(_) => return std::ptr::null_mut(),
        },
        _ => return std::ptr::null_mut(),
    };
    let rt_affinity = has_rt_affinity.then_some(CoreId { id: rt_affinity });
    into_handle(ClientConfig {
        low_latency,
        timeout_cycles,
        max_inflight,
        max_resync_rounds,
        reset_resend_cycles,
        rt_priority,
        rt_policy: RtSchedulePolicy::default(),
        rt_affinity,
        validate_state,
        ..Default::default()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_free(config: *mut ClientConfig) {
    unsafe { drop_handle(config) }
}

pub enum Pending {
    Pattern {
        emissions: Vec<DevicePattern>,
        bank: PatternBank,
        transition_mode: TransitionMode,
    },
    Modulation {
        divider: u16,
        data: Vec<u8>,
        bank: ModulationBank,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    },
    WritePatternBuffer {
        bank: PatternBank,
        index: u16,
        emissions: Vec<DevicePattern>,
    },
    WriteFociBuffer {
        bank: PatternBank,
        index_offset: usize,
        points: FociPoints,
    },
    WritePatternCompressed {
        bank: PatternBank,
        index: u32,
        format: PatternCompression,
        patterns: Vec<Vec<DevicePattern>>,
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
    ChangePatternBank {
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
    ChangeModulationBank {
        bank: ModulationBank,
        transition_mode: TransitionMode,
    },
    Clear,
    Synchronize,
    Nop,
    ForceFan(bool),
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
        patterns: Vec<Vec<DevicePattern>>,
        bank: PatternBank,
        mode: PatternStmMode,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    },
    Each(Vec<Option<Pending>>),
}

fn to_pattern_compression(v: u8) -> PatternCompression {
    match v {
        2 => PatternCompression::PhaseHalf,
        _ => PatternCompression::PhaseFull,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_pattern(
    bank: u8,
    pattern_buffer: *const PatternBuffer,
    transition_mode: u8,
    transition_value: u64,
    transition_margin_ns: u32,
) -> *mut Pending {
    if pattern_buffer.is_null() {
        return std::ptr::null_mut();
    }

    into_handle(Pending::Pattern {
        emissions: unsafe { &*pattern_buffer }.0.clone(),
        bank: to_pattern_bank(bank),
        transition_mode: to_transition_mode(
            transition_mode,
            transition_value,
            transition_margin_ns,
        ),
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
    transition_margin_ns: u32,
) -> *mut Pending {
    if sampling_config.is_null() || modulation_buffer.is_null() {
        return std::ptr::null_mut();
    }

    let Ok(divider) = unsafe { &*sampling_config }.divide() else {
        return std::ptr::null_mut();
    };
    let data = unsafe { &*modulation_buffer }.0.clone();
    into_handle(Pending::Modulation {
        divider,
        data,
        bank: to_modulation_bank(bank),
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode: to_transition_mode(
            transition_mode,
            transition_value,
            transition_margin_ns,
        ),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_pattern_buffer(
    bank: u8,
    index: u16,
    pattern_buffer: *const PatternBuffer,
) -> *mut Pending {
    if pattern_buffer.is_null() {
        return std::ptr::null_mut();
    }

    let emissions = unsafe { &*pattern_buffer }.0.clone();
    into_handle(Pending::WritePatternBuffer {
        bank: to_pattern_bank(bank),
        index,
        emissions,
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
    if points.is_null() || intensities.is_null() || num_foci == 0 {
        return std::ptr::null_mut();
    }

    let n = usize::from(num_foci);
    let points = unsafe { std::slice::from_raw_parts(points, num_samples * n) };
    let intensities = unsafe { std::slice::from_raw_parts(intensities, num_samples) };
    let samples = points
        .chunks_exact(n)
        .zip(intensities)
        .map(|(chunk, intensity)| FociSample {
            intensity: Intensity(*intensity),
            points: chunk
                .iter()
                .map(|p| {
                    ControlPoint::new(
                        Point3::new(p.point[0], p.point[1], p.point[2]),
                        Phase(p.phase_offset),
                    )
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let Some(points) = FociPoints::from_samples(&samples, n) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::WriteFociBuffer {
        bank: to_pattern_bank(bank),
        index_offset: index_offset as usize,
        points,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_pattern_compressed(
    bank: u8,
    index: u32,
    format: u8,
    patterns: *const *const PatternBuffer,
    num_patterns: usize,
) -> *mut Pending {
    if patterns.is_null() || num_patterns == 0 {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(patterns, num_patterns) };
    if slice.iter().any(|p| p.is_null()) {
        return std::ptr::null_mut();
    }
    let patterns = slice.iter().map(|p| unsafe { &**p }.0.clone()).collect();
    into_handle(Pending::WritePatternCompressed {
        bank: to_pattern_bank(bank),
        index,
        format: to_pattern_compression(format),
        patterns,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_pattern_compression_per_frame(format: u8) -> usize {
    to_pattern_compression(format).per_frame()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_config_pattern(
    bank: u8,
    sampling_config: *const SamplingConfig,
    size: u32,
    rep: u16,
) -> *mut Pending {
    if sampling_config.is_null() {
        return std::ptr::null_mut();
    }
    into_handle(Pending::ConfigPattern {
        bank: to_pattern_bank(bank),
        config: *unsafe { &*sampling_config },
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
    if sampling_config.is_null() {
        return std::ptr::null_mut();
    }
    into_handle(Pending::ConfigFociStm {
        bank: to_pattern_bank(bank),
        config: *unsafe { &*sampling_config },
        size,
        num_foci,
        sound_speed: Velocity::from_m_s(sound_speed_m_s),
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_change_pattern_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
    transition_margin_ns: u32,
) -> *mut Pending {
    into_handle(Pending::ChangePatternBank {
        bank: to_pattern_bank(bank),
        transition_mode: to_transition_mode(
            transition_mode,
            transition_value,
            transition_margin_ns,
        ),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_write_modulation_buffer(
    bank: u8,
    offset: u32,
    modulation_buffer: *const ModulationBuffer,
) -> *mut Pending {
    if modulation_buffer.is_null() {
        return std::ptr::null_mut();
    }

    let data = unsafe { &*modulation_buffer }.0.clone();
    into_handle(Pending::WriteModulationBuffer {
        bank: to_modulation_bank(bank),
        offset,
        data,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_config_modulation(
    bank: u8,
    sampling_config: *const SamplingConfig,
    size: u32,
    rep: u16,
) -> *mut Pending {
    if sampling_config.is_null() {
        return std::ptr::null_mut();
    }
    into_handle(Pending::ConfigModulation {
        bank: to_modulation_bank(bank),
        config: *unsafe { &*sampling_config },
        size,
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_change_modulation_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
    transition_margin_ns: u32,
) -> *mut Pending {
    into_handle(Pending::ChangeModulationBank {
        bank: to_modulation_bank(bank),
        transition_mode: to_transition_mode(
            transition_mode,
            transition_value,
            transition_margin_ns,
        ),
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
pub extern "C" fn autd3_op_nop() -> *mut Pending {
    into_handle(Pending::Nop)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_force_fan(value: bool) -> *mut Pending {
    into_handle(Pending::ForceFan(value))
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
    if outputs.is_null() {
        return std::ptr::null_mut();
    }

    let outputs = unsafe { std::slice::from_raw_parts(outputs, 4) };
    into_handle(Pending::SetGpioOut([
        to_gpio_out(&outputs[0]),
        to_gpio_out(&outputs[1]),
        to_gpio_out(&outputs[2]),
        to_gpio_out(&outputs[3]),
    ]))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_emulate_gpio_in(values: *const u8) -> *mut Pending {
    if values.is_null() {
        return std::ptr::null_mut();
    }

    let values = unsafe { std::slice::from_raw_parts(values, 4) };
    into_handle(Pending::EmulateGpioIn([
        values[0] != 0,
        values[1] != 0,
        values[2] != 0,
        values[3] != 0,
    ]))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_output_mask(
    masks: *const u8,
    lens: *const usize,
    num_devices: usize,
) -> *mut Pending {
    if masks.is_null() || lens.is_null() {
        return std::ptr::null_mut();
    }

    let lens = unsafe { std::slice::from_raw_parts(lens, num_devices) };
    let slice = unsafe { std::slice::from_raw_parts(masks, lens.iter().sum()) };
    let mut offset = 0;
    let masks = lens
        .iter()
        .map(|&len| {
            let device = &slice[offset..offset + len];
            offset += len;
            device.iter().map(|&src| src != 0).collect()
        })
        .collect();
    into_handle(Pending::SetOutputMask(masks))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_phase_correction(
    phases: *const u8,
    lens: *const usize,
    num_devices: usize,
) -> *mut Pending {
    if phases.is_null() || lens.is_null() {
        return std::ptr::null_mut();
    }

    let lens = unsafe { std::slice::from_raw_parts(lens, num_devices) };
    let slice = unsafe { std::slice::from_raw_parts(phases, lens.iter().sum()) };
    let mut offset = 0;
    let phases = lens
        .iter()
        .map(|&len| {
            let device = &slice[offset..offset + len];
            offset += len;
            device.iter().map(|&src| Phase(src)).collect()
        })
        .collect();
    into_handle(Pending::SetPhaseCorrection(phases))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_pulse_width_table(table: *const u16) -> *mut Pending {
    if table.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(table, PWE_TABLE_SIZE) };
    let mut t = Box::new([PulseWidth::new(0); PWE_TABLE_SIZE]);
    for (dst, &src) in t.iter_mut().zip(slice.iter()) {
        *dst = PulseWidth::new(src);
    }
    into_handle(Pending::SetPulseWidthTable(t))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_set_pulse_width_table_default_table(out: *mut u16) {
    if out.is_null() {
        return;
    }

    let table = SetPulseWidthTable::default_table();
    for (i, pw) in table.iter().enumerate() {
        unsafe { *out.add(i) = pw.pulse_width().unwrap_or(0) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_from_duty(duty: f32, out: *mut u16) -> bool {
    if out.is_null() {
        return false;
    }

    let Ok(value) = PulseWidth::from_duty(duty).pulse_width() else {
        return false;
    };

    unsafe { *out = value };
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_new(pulse_width: u16, out: *mut u16) -> bool {
    if out.is_null() {
        return false;
    }

    let Ok(value) = PulseWidth::new(pulse_width).pulse_width() else {
        return false;
    };

    unsafe { *out = value };
    true
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
    transition_margin_ns: u32,
) -> *mut Pending {
    if config.is_null() || points.is_null() || intensities.is_null() || num_foci == 0 {
        return std::ptr::null_mut();
    }

    let n = usize::from(num_foci);
    let points = unsafe { std::slice::from_raw_parts(points, num_samples * n) };
    let intensities = unsafe { std::slice::from_raw_parts(intensities, num_samples) };
    let samples = points
        .chunks_exact(n)
        .zip(intensities)
        .map(|(chunk, intensity)| FociSample {
            intensity: Intensity(*intensity),
            points: chunk
                .iter()
                .map(|p| {
                    ControlPoint::new(
                        Point3::new(p.point[0], p.point[1], p.point[2]),
                        Phase(p.phase_offset),
                    )
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let Some(points) = FociPoints::from_samples(&samples, n) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::FociStm {
        config: *unsafe { &*config },
        points,
        bank: to_pattern_bank(bank),
        sound_speed: sound_speed_m_s,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode: to_transition_mode(
            transition_mode,
            transition_value,
            transition_margin_ns,
        ),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_pattern_stm(
    config: *const StmConfig,
    patterns: *const *const PatternBuffer,
    num_patterns: usize,
    bank: u8,
    mode: u8,
    loop_rep: u16,
    transition_mode: u8,
    transition_value: u64,
    transition_margin_ns: u32,
) -> *mut Pending {
    if config.is_null() || patterns.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(patterns, num_patterns) };
    if slice.iter().any(|p| p.is_null()) {
        return std::ptr::null_mut();
    }
    let patterns = slice.iter().map(|p| unsafe { &**p }.0.clone()).collect();
    into_handle(Pending::PatternStm {
        config: *unsafe { &*config },
        patterns,
        bank: to_pattern_bank(bank),
        mode: to_pattern_stm_mode(mode),
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode: to_transition_mode(
            transition_mode,
            transition_value,
            transition_margin_ns,
        ),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_free(op: *mut Pending) {
    unsafe { drop_handle(op) }
}

#[allow(clippy::too_many_lines)]
fn pending_to_boxed(pending: &Pending) -> Option<BoxedCommand<'_>> {
    Some(match pending {
        Pending::Pattern {
            emissions,
            bank,
            transition_mode,
        } => Pattern {
            transition_mode: *transition_mode,
            ..Pattern::with_bank(*bank, emissions)
        }
        .boxed(),
        Pending::Modulation {
            divider,
            data,
            bank,
            loop_behavior,
            transition_mode,
        } => {
            let divider = NonZeroU16::new(*divider)?;
            Modulation {
                bank: *bank,
                config: SamplingConfig::new(divider),
                data,
                loop_behavior: *loop_behavior,
                transition_mode: *transition_mode,
            }
            .boxed()
        }
        Pending::WritePatternBuffer {
            bank,
            index,
            emissions,
        } => WritePatternBuffer {
            bank: *bank,
            index: usize::from(*index),
            emissions,
        }
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
        Pending::ChangePatternBank {
            bank,
            transition_mode,
        } => ChangePatternBank {
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
        Pending::ChangeModulationBank {
            bank,
            transition_mode,
        } => ChangeModulationBank {
            bank: *bank,
            transition_mode: *transition_mode,
        }
        .boxed(),
        Pending::Clear => Clear.boxed(),
        Pending::Synchronize => Synchronize.boxed(),
        Pending::Nop => Nop.boxed(),
        Pending::ForceFan(value) => ForceFan { value: *value }.boxed(),
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
        Pending::SetSilencerUpdateRate { intensity, phase } => SetSilencer::new(FixedUpdateRate {
            intensity: *intensity,
            phase: *phase,
        })
        .boxed(),
        Pending::SetSilencerDisable => SetSilencer::disable().boxed(),
        Pending::SetGpioOut(outputs) => SetGpioOut { outputs: *outputs }.boxed(),
        Pending::EmulateGpioIn(values) => EmulateGpioIn { values: *values }.boxed(),
        Pending::SetOutputMask(masks) => SetOutputMask { masks }.boxed(),
        Pending::SetPhaseCorrection(phases) => SetPhaseCorrection { phases }.boxed(),
        Pending::SetPulseWidthTable(t) => SetPulseWidthTable { table: t }.boxed(),
        _ => return None,
    })
}

pub struct DatagramBuilder {
    geometry: Arc<Geometry>,
    pending: Vec<Pending>,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagram_builder_new(
    geometry: *const Geometry,
) -> *mut DatagramBuilder {
    if geometry.is_null() {
        return std::ptr::null_mut();
    }

    into_handle(DatagramBuilder {
        geometry: Arc::new(unsafe { &*geometry }.clone()),
        pending: Vec::new(),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagram_builder_push(
    builder: *mut DatagramBuilder,
    op: *mut Pending,
) {
    if builder.is_null() || op.is_null() {
        return;
    }

    let op = unsafe { *Box::from_raw(op) };

    unsafe { &mut *builder }.pending.push(op);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagram_builder_push_each(
    builder: *mut DatagramBuilder,
    ops: *const *mut Pending,
    num_devices: usize,
) {
    if builder.is_null() || ops.is_null() {
        return;
    }

    let slice = unsafe { std::slice::from_raw_parts(ops, num_devices) };
    let devices: Vec<Option<Pending>> = slice
        .iter()
        .map(|&p| {
            if p.is_null() {
                None
            } else {
                Some(unsafe { *Box::from_raw(p) })
            }
        })
        .collect();
    unsafe { &mut *builder }
        .pending
        .push(Pending::Each(devices));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagram_builder_free(builder: *mut DatagramBuilder) {
    unsafe { drop_handle(builder) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_lines)]
pub unsafe extern "C" fn autd3_datagram_builder_build(
    builder: *const DatagramBuilder,
    client: *const ClientHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut Arc<Frames> {
    if builder.is_null() {
        unsafe { write_cstr(out_err, out_err_len, "null builder") };
        return std::ptr::null_mut();
    }

    let dc_offset_ns = if client.is_null() {
        0
    } else {
        unsafe { &*client }.0.dc_offset_ns()
    };
    let builder = unsafe { &*builder };
    let mut core = CoreDatagramBuilder::with_dc_offset(Arc::clone(&builder.geometry), dc_offset_ns);
    for pending in &builder.pending {
        match pending {
            Pending::Pattern {
                emissions,
                bank,
                transition_mode,
            } => {
                core.push(Pattern {
                    transition_mode: *transition_mode,
                    ..Pattern::with_bank(*bank, emissions)
                });
            }
            Pending::Modulation {
                divider,
                data,
                bank,
                loop_behavior,
                transition_mode,
            } => {
                let Some(divider) = NonZeroU16::new(*divider) else {
                    unsafe { write_cstr(out_err, out_err_len, "divider must be >= 1") };
                    return std::ptr::null_mut();
                };
                core.push(Modulation {
                    bank: *bank,
                    config: SamplingConfig::new(divider),
                    data,
                    loop_behavior: *loop_behavior,
                    transition_mode: *transition_mode,
                });
            }
            Pending::WritePatternBuffer {
                bank,
                index,
                emissions,
            } => {
                core.push(WritePatternBuffer {
                    bank: *bank,
                    index: usize::from(*index),
                    emissions,
                });
            }
            Pending::WriteFociBuffer {
                bank,
                index_offset,
                points,
            } => {
                points.push_write_foci_into(*bank, *index_offset, &mut core);
            }
            Pending::WritePatternCompressed {
                bank,
                index,
                format,
                patterns,
            } => {
                let mut arr: [Option<&[DevicePattern]>; 4] = [None; 4];
                for (slot, buf) in arr.iter_mut().zip(patterns.iter()) {
                    *slot = Some(buf.as_slice());
                }
                core.push(WritePatternCompressed {
                    bank: *bank,
                    index: usize::try_from(*index).unwrap_or(usize::MAX),
                    format: *format,
                    patterns: arr,
                });
            }
            Pending::ConfigPattern {
                bank,
                config,
                size,
                loop_behavior,
            } => {
                core.push(ConfigPattern {
                    bank: *bank,
                    config: *config,
                    size: usize::try_from(*size).unwrap_or(usize::MAX),
                    loop_behavior: *loop_behavior,
                });
            }
            Pending::ConfigFociStm {
                bank,
                config,
                size,
                num_foci,
                sound_speed,
                loop_behavior,
            } => {
                core.push(ConfigFociStm {
                    bank: *bank,
                    config: *config,
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
                core.push(ChangePatternBank {
                    bank: *bank,
                    transition_mode: *transition_mode,
                });
            }
            Pending::WriteModulationBuffer { bank, offset, data } => {
                core.push(WriteModulationBuffer {
                    bank: *bank,
                    offset: usize::try_from(*offset).unwrap_or(usize::MAX),
                    data,
                });
            }
            Pending::ConfigModulation {
                bank,
                config,
                size,
                loop_behavior,
            } => {
                core.push(ConfigModulation {
                    bank: *bank,
                    config: *config,
                    size: usize::try_from(*size).unwrap_or(usize::MAX),
                    loop_behavior: *loop_behavior,
                });
            }
            Pending::ChangeModulationBank {
                bank,
                transition_mode,
            } => {
                core.push(ChangeModulationBank {
                    bank: *bank,
                    transition_mode: *transition_mode,
                });
            }
            Pending::Clear => {
                core.push(Clear);
            }
            Pending::Synchronize => {
                core.push(Synchronize);
            }
            Pending::Nop => {
                core.push(Nop);
            }
            Pending::ForceFan(value) => {
                core.push(ForceFan { value: *value });
            }
            Pending::SetSilencerCompletion {
                intensity,
                phase,
                strict,
            } => {
                core.push(SetSilencer::new(FixedCompletionTime {
                    intensity: *intensity,
                    phase: *phase,
                    strict_mode: *strict,
                }));
            }
            Pending::SetSilencerUpdateRate { intensity, phase } => {
                core.push(SetSilencer::new(FixedUpdateRate {
                    intensity: *intensity,
                    phase: *phase,
                }));
            }
            Pending::SetSilencerDisable => {
                core.push(SetSilencer::disable());
            }
            Pending::SetGpioOut(outputs) => {
                core.push(SetGpioOut { outputs: *outputs });
            }
            Pending::EmulateGpioIn(values) => {
                core.push(EmulateGpioIn { values: *values });
            }
            Pending::SetOutputMask(masks) => {
                core.push(SetOutputMask { masks });
            }
            Pending::SetPhaseCorrection(phases) => {
                core.push(SetPhaseCorrection { phases });
            }
            Pending::SetPulseWidthTable(t) => {
                core.push(SetPulseWidthTable { table: t });
            }
            Pending::FociStm {
                config,
                points,
                bank,
                sound_speed,
                loop_behavior,
                transition_mode,
            } => {
                let option = FociStmOption {
                    bank: *bank,
                    sound_speed: Velocity::from_m_s(*sound_speed),
                    loop_behavior: *loop_behavior,
                    transition_mode: *transition_mode,
                };
                points.push_into(*config, option, &mut core);
            }
            Pending::PatternStm {
                config,
                patterns,
                bank,
                mode,
                loop_behavior,
                transition_mode,
            } => {
                core.push(PatternStm::new(
                    *config,
                    patterns,
                    PatternStmOption {
                        bank: *bank,
                        mode: *mode,
                        loop_behavior: *loop_behavior,
                        transition_mode: *transition_mode,
                    },
                ));
            }
            Pending::Each(devices) => {
                core.push_each::<BoxedCommand, _>(|device| {
                    devices
                        .get(device.idx())
                        .and_then(Option::as_ref)
                        .and_then(pending_to_boxed)
                });
            }
        }
    }
    match core.build() {
        Ok(datagrams) => into_handle(Arc::new(datagrams)),
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagrams_num_frames(datagrams: *const Arc<Frames>) -> usize {
    if datagrams.is_null() {
        return 0;
    }

    unsafe { &*datagrams }.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagrams_free(datagrams: *mut Arc<Frames>) {
    unsafe { drop_handle(datagrams) }
}

pub struct ClientHandle(Box<dyn ClientBackend>);

pub struct CheckerHandle(Box<dyn CheckerBackend>);

pub struct StringArray(Vec<CString>);

pub struct ByteArray(Vec<u8>);

pub struct LinkStatus {
    devices: Vec<DeviceState>,
    recoveries: u64,
}

pub(crate) fn to_cstrings(values: Vec<String>) -> Vec<CString> {
    values
        .into_iter()
        .map(|s| CString::new(s.replace('\0', " ")).unwrap_or_default())
        .collect()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_open(
    geometry: *const Geometry,
    link: *mut ClientOpener,
    config: *const ClientConfig,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if geometry.is_null() || link.is_null() || config.is_null() {
        ctx.err("null argument");
        return;
    }

    let opener = unsafe { *Box::from_raw(link) };

    let geometry = unsafe { &*geometry }.clone();
    let config = *unsafe { &*config };
    let fut = opener(geometry, config);
    runtime().spawn(async move {
        match fut.await {
            Ok(backend) => ctx.ok(into_handle(ClientHandle(backend)).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_num_devices(client: *const ClientHandle) -> usize {
    if client.is_null() {
        return 0;
    }

    unsafe { &*client }.0.num_devices()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send_checked(
    client: *const ClientHandle,
    datagrams: *const Arc<Frames>,
    frame: i64,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() || datagrams.is_null() {
        ctx.err("null argument");
        return;
    }

    let datagrams = unsafe { &*datagrams }.clone();
    let frame = usize::try_from(frame).ok();
    let fut = unsafe { &*client }.0.send_checked(datagrams, frame);
    runtime().spawn(async move {
        match fut.await {
            Ok(()) => ctx.ok(std::ptr::null_mut()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

pub struct ResponseToken(ResponseTokenData);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send(
    client: *const ClientHandle,
    datagrams: *const Arc<Frames>,
    frame: i64,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() || datagrams.is_null() {
        ctx.err("null argument");
        return;
    }

    let datagrams = unsafe { &*datagrams }.clone();
    let frame = usize::try_from(frame).ok();
    let fut = unsafe { &*client }.0.send(datagrams, frame);
    runtime().spawn(async move {
        match fut.await {
            Ok(token) => ctx.ok(into_handle(ResponseToken(token)).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_token_await(
    token: *mut ResponseToken,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if token.is_null() {
        ctx.err("null token");
        return;
    }

    let token = unsafe { *Box::from_raw(token) };
    let fut = token.0.0;
    runtime().spawn(async move {
        match fut.await {
            Ok(response) => ctx.ok(into_handle(ByteArray(response.data().to_vec())).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
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
) -> bool {
    let response = if data.is_null() || len == 0 {
        Response::default()
    } else {
        // SAFETY: the caller guarantees `data` points to `len` readable bytes.
        Response::from_slice(unsafe { std::slice::from_raw_parts(data, len) })
    };
    match response.check() {
        Ok(()) => true,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_firmware_version(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() {
        ctx.err("null client");
        return;
    }

    let fut = unsafe { &*client }.0.read_firmware_version();
    runtime().spawn(async move {
        match fut.await {
            Ok(versions) => ctx.ok(into_handle(StringArray(to_cstrings(versions))).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_fpga_state(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() {
        ctx.err("null client");
        return;
    }

    let fut = unsafe { &*client }.0.read_fpga_state();
    runtime().spawn(async move {
        match fut.await {
            Ok(states) => ctx.ok(into_handle(ByteArray(states)).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_telemetry(
    client: *const ClientHandle,
    counter: u8,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() {
        ctx.err("null client");
        return;
    }
    let Some(counter) = to_telemetry(counter) else {
        ctx.err("unknown telemetry counter");
        return;
    };

    let fut = unsafe { &*client }.0.read_telemetry(counter);
    runtime().spawn(async move {
        match fut.await {
            Ok(values) => ctx.ok(into_handle(ByteArray(values)).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_error_detail(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() {
        ctx.err("null client");
        return;
    }

    let fut = unsafe { &*client }.0.read_error_detail();
    runtime().spawn(async move {
        match fut.await {
            Ok(detail) => ctx.ok(into_handle(ByteArray(detail)).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_byte_array_len(array: *const ByteArray) -> usize {
    if array.is_null() {
        return 0;
    }

    unsafe { &*array }.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_byte_array_data(array: *const ByteArray) -> *const u8 {
    if array.is_null() {
        return std::ptr::null();
    }

    unsafe { &*array }.0.as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_byte_array_free(array: *mut ByteArray) {
    unsafe { drop_handle(array) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_checker(client: *const ClientHandle) -> *mut CheckerHandle {
    if client.is_null() {
        return std::ptr::null_mut();
    }

    into_handle(CheckerHandle(unsafe { &*client }.0.checker()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_checker_check(
    checker: *const CheckerHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if checker.is_null() {
        ctx.err("null checker");
        return;
    }

    let fut = unsafe { &*checker }.0.check();
    runtime().spawn(async move {
        match fut.await {
            Ok(status) => {
                let status = LinkStatus {
                    devices: status.devices,
                    recoveries: status.recoveries,
                };
                ctx.ok(into_handle(status).cast());
            }
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_checker_free(checker: *mut CheckerHandle) {
    unsafe { drop_handle(checker) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_stop(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() {
        ctx.err("null client");
        return;
    }

    let fut = unsafe { &*client }.0.stop();
    runtime().spawn(async move {
        match fut.await {
            Ok(()) => ctx.ok(std::ptr::null_mut()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_close(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() {
        ctx.err("null client");
        return;
    }

    let fut = unsafe { &*client }.0.close();
    runtime().spawn(async move {
        match fut.await {
            Ok(()) => ctx.ok(std::ptr::null_mut()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_free(client: *mut ClientHandle) {
    unsafe { drop_handle(client) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_string_array_len(array: *const StringArray) -> usize {
    if array.is_null() {
        return 0;
    }

    unsafe { &*array }.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_string_array_get(
    array: *const StringArray,
    index: usize,
) -> *const c_char {
    if array.is_null() {
        return std::ptr::null();
    }

    unsafe { &*array }
        .0
        .get(index)
        .map_or(std::ptr::null(), |s| s.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_string_array_free(array: *mut StringArray) {
    unsafe { drop_handle(array) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_recoveries(status: *const LinkStatus) -> u64 {
    if status.is_null() {
        return 0;
    }

    unsafe { &*status }.recoveries
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_num_devices(status: *const LinkStatus) -> usize {
    if status.is_null() {
        return 0;
    }

    unsafe { &*status }.devices.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_device_state(
    status: *const LinkStatus,
    index: usize,
    out_kind: *mut u8,
    out_bits: *mut u8,
) -> bool {
    if status.is_null() || out_kind.is_null() || out_bits.is_null() {
        return false;
    }

    let Some(state) = unsafe { &*status }.devices.get(index) else {
        return false;
    };
    let (kind, bits) = match state {
        DeviceState::Op => (0, 0),
        DeviceState::SafeOp => (1, 0),
        DeviceState::SafeOpError => (2, 0),
        DeviceState::Lost => (3, 0),
        DeviceState::Other(bits) => (4, *bits),
    };

    unsafe {
        *out_kind = kind;
        *out_bits = bits;
    }
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_free(status: *mut LinkStatus) {
    unsafe { drop_handle(status) }
}
