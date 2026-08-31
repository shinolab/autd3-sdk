use std::ffi::{CString, c_char, c_void};
use std::num::{NonZeroU16, NonZeroU32, NonZeroUsize};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, CheckerBackend, ClientBackend, ClientOpener,
    CompletionCallback, CompletionCtx, DevicePattern, ModulationBuffer, PatternBuffer,
    ResponseTokenData, drop_handle, handle_mut, handle_ref, into_handle, slice_mut, slice_ref,
    take_handle, to_rt_policy, to_rt_priority, write_cstr, write_out,
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
    Response, UnitVector3, Vector3, Velocity,
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

fn to_telemetry(counter: u8) -> Option<Telemetry> {
    match counter {
        0x00 => Some(Telemetry::FifoDrop),
        0x01 => Some(Telemetry::Dedup),
        0x02 => Some(Telemetry::SeqMismatch),
        0x03 => Some(Telemetry::DispatchError),
        0x04 => Some(Telemetry::Processed),
        0x05 => Some(Telemetry::Failsafe),
        0x06 => Some(Telemetry::SyncResync),
        _ => None,
    }
}

pub(crate) fn to_transition_mode(mode: u8, value: u64, margin_ns: u32) -> Option<TransitionMode> {
    match mode {
        0x00 => Some(TransitionMode::SyncIdx),
        0x01 => Some(TransitionMode::SysTime {
            time: DcSysTime::from_nanos(value),
            margin: (margin_ns != 0).then(|| Duration::from_nanos(u64::from(margin_ns))),
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
        10 => Some(GpioOut::SysTimeEq(DcSysTime::from_nanos(g.value))),
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

fn to_pattern_stm_mode(mode: u8) -> Option<PatternStmMode> {
    match mode {
        0 => Some(PatternStmMode::PhaseIntensityFull),
        1 => Some(PatternStmMode::PhaseFull),
        2 => Some(PatternStmMode::PhaseHalf),
        _ => None,
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
    match Duration::try_from_secs_f32(secs) {
        Ok(period) => into_handle(StmConfig::new(period)),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_period_nearest(secs: f32) -> *mut StmConfig {
    match Duration::try_from_secs_f32(secs) {
        Ok(period) => into_handle(StmConfig::new(Nearest(period))),
        Err(_) => std::ptr::null_mut(),
    }
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
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return -1;
    };

    if u32::try_from(size).is_err() {
        return -1;
    }

    let Ok(value) = config.into_sampling_config(size).divide() else {
        return -1;
    };

    if unsafe { write_out(out, value) } != AUTD3_OK {
        return -1;
    }
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

client_config_setter!(autd3_client_config_set_low_latency, low_latency, bool);
client_config_setter!(autd3_client_config_set_validate_state, validate_state, bool);
client_config_setter!(
    autd3_client_config_set_require_supported_firmware,
    require_supported_firmware,
    bool
);
client_config_setter!(
    autd3_client_config_set_timeout_cycles,
    timeout_cycles,
    u32,
    NonZeroU32
);
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
client_config_setter!(
    autd3_client_config_set_reset_resend_cycles,
    reset_resend_cycles,
    u32,
    NonZeroU32
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_set_rt_priority(
    config: *mut ClientConfig,
    mode: u8,
    value: u8,
) -> i32 {
    let (Some(config), Some(rt_priority)) =
        (unsafe { handle_mut(config) }, to_rt_priority(mode, value))
    else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    config.rt_priority = rt_priority;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_set_rt_policy(
    config: *mut ClientConfig,
    value: u8,
) -> i32 {
    let (Some(config), Some(rt_policy)) = (unsafe { handle_mut(config) }, to_rt_policy(value))
    else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    config.rt_policy = rt_policy;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_set_rt_affinity(
    config: *mut ClientConfig,
    has_affinity: bool,
    core_id: usize,
) -> i32 {
    let Some(config) = (unsafe { handle_mut(config) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    config.rt_affinity = has_affinity.then_some(CoreId { id: core_id });
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_free(config: *mut ClientConfig) {
    unsafe { drop_handle(config) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_opener_free(opener: *mut ClientOpener) {
    unsafe { drop_handle(opener) }
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

fn to_pattern_compression(v: u8) -> Option<PatternCompression> {
    match v {
        1 => Some(PatternCompression::PhaseFull),
        2 => Some(PatternCompression::PhaseHalf),
        _ => None,
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
    let Some(pattern_buffer) = (unsafe { handle_ref(pattern_buffer) }) else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        to_transition_mode(transition_mode, transition_value, transition_margin_ns),
    ) else {
        return std::ptr::null_mut();
    };

    into_handle(Pending::Pattern {
        emissions: pattern_buffer.0.clone(),
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
    transition_margin_ns: u32,
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
        to_transition_mode(transition_mode, transition_value, transition_margin_ns),
    ) else {
        return std::ptr::null_mut();
    };

    let Ok(divider) = sampling_config.divide() else {
        return std::ptr::null_mut();
    };
    let data = modulation_buffer.0.clone();
    into_handle(Pending::Modulation {
        divider,
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
    pattern_buffer: *const PatternBuffer,
) -> *mut Pending {
    let Some(pattern_buffer) = (unsafe { handle_ref(pattern_buffer) }) else {
        return std::ptr::null_mut();
    };
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };

    let emissions = pattern_buffer.0.clone();
    into_handle(Pending::WritePatternBuffer {
        bank,
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
    if num_foci == 0 {
        return std::ptr::null_mut();
    }
    let Some(bank) = to_pattern_bank(bank) else {
        return std::ptr::null_mut();
    };

    let n = usize::from(num_foci);
    let (Some(points), Some(intensities)) =
        (unsafe { slice_ref(points, num_samples * n) }, unsafe {
            slice_ref(intensities, num_samples)
        })
    else {
        return std::ptr::null_mut();
    };
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
        bank,
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
    if num_patterns == 0 {
        return std::ptr::null_mut();
    }
    let (Some(bank), Some(format)) = (to_pattern_bank(bank), to_pattern_compression(format)) else {
        return std::ptr::null_mut();
    };

    let Some(slice) = (unsafe { slice_ref(patterns, num_patterns) }) else {
        return std::ptr::null_mut();
    };
    let mut patterns = Vec::with_capacity(slice.len());
    for p in slice {
        let Some(pattern) = (unsafe { handle_ref(*p) }) else {
            return std::ptr::null_mut();
        };
        patterns.push(pattern.0.clone());
    }
    into_handle(Pending::WritePatternCompressed {
        bank,
        index,
        format,
        patterns,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pattern_compression_per_frame(format: u8, out: *mut usize) -> i32 {
    let Some(format) = to_pattern_compression(format) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, format.per_frame()) }
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
pub extern "C" fn autd3_op_change_pattern_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
    transition_margin_ns: u32,
) -> *mut Pending {
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        to_transition_mode(transition_mode, transition_value, transition_margin_ns),
    ) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::ChangePatternBank {
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
pub extern "C" fn autd3_op_change_modulation_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
    transition_margin_ns: u32,
) -> *mut Pending {
    let (Some(bank), Some(transition_mode)) = (
        to_modulation_bank(bank),
        to_transition_mode(transition_mode, transition_value, transition_margin_ns),
    ) else {
        return std::ptr::null_mut();
    };
    into_handle(Pending::ChangeModulationBank {
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

fn total_len(lens: &[usize]) -> Option<usize> {
    lens.iter()
        .try_fold(0usize, |acc, &len| acc.checked_add(len))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_output_mask(
    masks: *const u8,
    lens: *const usize,
    num_devices: usize,
) -> *mut Pending {
    let Some(lens) = (unsafe { slice_ref(lens, num_devices) }) else {
        return std::ptr::null_mut();
    };
    let Some(total) = total_len(lens) else {
        return std::ptr::null_mut();
    };
    let Some(slice) = (unsafe { slice_ref(masks, total) }) else {
        return std::ptr::null_mut();
    };
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
    let Some(lens) = (unsafe { slice_ref(lens, num_devices) }) else {
        return std::ptr::null_mut();
    };
    let Some(total) = total_len(lens) else {
        return std::ptr::null_mut();
    };
    let Some(slice) = (unsafe { slice_ref(phases, total) }) else {
        return std::ptr::null_mut();
    };
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
    let Some(slice) = (unsafe { slice_ref(table, PWE_TABLE_SIZE) }) else {
        return std::ptr::null_mut();
    };

    let mut t = Box::new([PulseWidth::new(0); PWE_TABLE_SIZE]);
    for (dst, &src) in t.iter_mut().zip(slice.iter()) {
        *dst = PulseWidth::new(src);
    }
    into_handle(Pending::SetPulseWidthTable(t))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_set_pulse_width_table_default_table(out: *mut u16) {
    let table = SetPulseWidthTable::default_table();
    let Some(out) = (unsafe { slice_mut(out, table.len()) }) else {
        return;
    };

    for (dst, pw) in out.iter_mut().zip(table.iter()) {
        *dst = pw.pulse_width().unwrap_or(0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_from_duty(duty: f32, out: *mut u16) -> bool {
    let Ok(value) = PulseWidth::from_duty(duty).pulse_width() else {
        return false;
    };

    unsafe { write_out(out, value) == AUTD3_OK }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_new(pulse_width: u16, out: *mut u16) -> bool {
    let Ok(value) = PulseWidth::new(pulse_width).pulse_width() else {
        return false;
    };

    unsafe { write_out(out, value) == AUTD3_OK }
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
    if num_foci == 0 {
        return std::ptr::null_mut();
    }
    let Some(config) = (unsafe { handle_ref(config) }) else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(transition_mode)) = (
        to_pattern_bank(bank),
        to_transition_mode(transition_mode, transition_value, transition_margin_ns),
    ) else {
        return std::ptr::null_mut();
    };

    let n = usize::from(num_foci);
    let (Some(points), Some(intensities)) =
        (unsafe { slice_ref(points, num_samples * n) }, unsafe {
            slice_ref(intensities, num_samples)
        })
    else {
        return std::ptr::null_mut();
    };
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
    patterns: *const *const PatternBuffer,
    num_patterns: usize,
    bank: u8,
    mode: u8,
    loop_rep: u16,
    transition_mode: u8,
    transition_value: u64,
    transition_margin_ns: u32,
) -> *mut Pending {
    let (Some(config), Some(slice)) = (unsafe { handle_ref(config) }, unsafe {
        slice_ref(patterns, num_patterns)
    }) else {
        return std::ptr::null_mut();
    };
    let (Some(bank), Some(mode), Some(transition_mode)) = (
        to_pattern_bank(bank),
        to_pattern_stm_mode(mode),
        to_transition_mode(transition_mode, transition_value, transition_margin_ns),
    ) else {
        return std::ptr::null_mut();
    };

    let mut patterns = Vec::with_capacity(slice.len());
    for p in slice {
        let Some(pattern) = (unsafe { handle_ref(*p) }) else {
            return std::ptr::null_mut();
        };
        patterns.push(pattern.0.clone());
    }
    into_handle(Pending::PatternStm {
        config: *config,
        patterns,
        bank,
        mode,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode,
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
        Pending::WriteFociBuffer {
            bank,
            index_offset,
            points,
        } => points.boxed_write_foci(*bank, *index_offset),
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
            WritePatternCompressed {
                bank: *bank,
                index: usize::try_from(*index).unwrap_or(usize::MAX),
                format: *format,
                patterns: arr,
            }
            .boxed()
        }
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
            patterns,
            bank,
            mode,
            loop_behavior,
            transition_mode,
        } => PatternStm::new(
            *config,
            patterns,
            PatternStmOption {
                bank: *bank,
                mode: *mode,
                loop_behavior: *loop_behavior,
                transition_mode: *transition_mode,
            },
        )
        .boxed(),
        Pending::Each(_) => return None,
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
    let Some(geometry) = (unsafe { handle_ref(geometry) }) else {
        return std::ptr::null_mut();
    };

    into_handle(DatagramBuilder {
        geometry: Arc::new(geometry.clone()),
        pending: Vec::new(),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagram_builder_push(
    builder: *mut DatagramBuilder,
    op: *mut Pending,
) -> i32 {
    let Some(builder) = (unsafe { handle_mut(builder) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(op) = (unsafe { take_handle(op) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    builder.pending.push(op);
    AUTD3_OK
}

pub(crate) unsafe fn take_each(
    ops: *const *mut Pending,
    num_devices: usize,
) -> Option<Vec<Option<Pending>>> {
    let slice = unsafe { slice_ref(ops, num_devices) }?;
    if slice
        .iter()
        .filter_map(|&p| unsafe { handle_ref(p.cast_const()) })
        .any(|pending| matches!(pending, Pending::Each(_)))
    {
        return None;
    }
    Some(slice.iter().map(|&p| unsafe { take_handle(p) }).collect())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagram_builder_push_each(
    builder: *mut DatagramBuilder,
    ops: *const *mut Pending,
    num_devices: usize,
) -> i32 {
    let Some(builder) = (unsafe { handle_mut(builder) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(devices) = (unsafe { take_each(ops, num_devices) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };

    builder.pending.push(Pending::Each(devices));
    AUTD3_OK
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
    let Some(builder) = (unsafe { handle_ref(builder) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null builder") };
        return std::ptr::null_mut();
    };

    let dc_offset_ns =
        unsafe { handle_ref(client) }.map_or(0, |client: &ClientHandle| client.0.dc_offset_ns());
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
    let Some(datagrams) = (unsafe { handle_ref::<Arc<Frames>>(datagrams) }) else {
        return 0;
    };

    datagrams.len()
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
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let (Some(geometry), Some(config)) = (unsafe { handle_ref(geometry) }, unsafe {
        handle_ref::<ClientConfig>(config)
    }) else {
        ctx.invalid_argument("null argument");
        return;
    };
    let Some(opener) = (unsafe { take_handle(link) }) else {
        ctx.invalid_argument("null argument");
        return;
    };

    let fut = opener(geometry.clone(), *config);
    runtime().spawn(async move {
        match fut.await {
            Ok(backend) => ctx.ok(into_handle(ClientHandle(backend)).cast()),
            Err(e) => ctx.err_of(&e),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_num_devices(client: *const ClientHandle) -> usize {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return 0;
    };

    client.0.num_devices()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_send_checked(
    client: *const ClientHandle,
    datagrams: *const Arc<Frames>,
    frame: i64,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let (Some(client), Some(datagrams)) = (unsafe { handle_ref(client) }, unsafe {
        handle_ref::<Arc<Frames>>(datagrams)
    }) else {
        ctx.err("null argument");
        return;
    };

    let datagrams = datagrams.clone();
    let frame = usize::try_from(frame).ok();
    let fut = client.0.send_checked(datagrams, frame);
    runtime().spawn(async move {
        match fut.await {
            Ok(()) => ctx.ok(std::ptr::null_mut()),
            Err(e) => ctx.err_of(&e),
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
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let (Some(client), Some(datagrams)) = (unsafe { handle_ref(client) }, unsafe {
        handle_ref::<Arc<Frames>>(datagrams)
    }) else {
        ctx.err("null argument");
        return;
    };

    let datagrams = datagrams.clone();
    let frame = usize::try_from(frame).ok();
    let fut = client.0.send(datagrams, frame);
    runtime().spawn(async move {
        match fut.await {
            Ok(token) => ctx.ok(into_handle(ResponseToken(token)).cast()),
            Err(e) => ctx.err_of(&e),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_response_token_await(
    token: *mut ResponseToken,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    if token.is_null() {
        ctx.err("null token");
        return;
    }

    let token = unsafe { *Box::from_raw(token) };
    let fut = token.0.0;
    runtime().spawn(async move {
        match fut.await {
            Ok(response) => ctx.ok(into_handle(ByteArray(response.data().to_vec())).cast()),
            Err(e) => ctx.err_of(&e),
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
    let response = match unsafe { slice_ref(data, len) } {
        Some(data) if !data.is_empty() => Response::from_slice(data),
        _ => Response::default(),
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
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(client) = (unsafe { handle_ref(client) }) else {
        ctx.err("null client");
        return;
    };

    let fut = client.0.read_firmware_version();
    runtime().spawn(async move {
        match fut.await {
            Ok(versions) => ctx.ok(into_handle(StringArray(to_cstrings(versions))).cast()),
            Err(e) => ctx.err_of(&e),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_fpga_state(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(client) = (unsafe { handle_ref(client) }) else {
        ctx.err("null client");
        return;
    };

    let fut = client.0.read_fpga_state();
    runtime().spawn(async move {
        match fut.await {
            Ok(states) => ctx.ok(into_handle(ByteArray(states)).cast()),
            Err(e) => ctx.err_of(&e),
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
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(client) = (unsafe { handle_ref(client) }) else {
        ctx.err("null client");
        return;
    };
    let Some(counter) = to_telemetry(counter) else {
        ctx.err("unknown telemetry counter");
        return;
    };

    let fut = client.0.read_telemetry(counter);
    runtime().spawn(async move {
        match fut.await {
            Ok(values) => ctx.ok(into_handle(ByteArray(values)).cast()),
            Err(e) => ctx.err_of(&e),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_read_error_detail(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(client) = (unsafe { handle_ref(client) }) else {
        ctx.err("null client");
        return;
    };

    let fut = client.0.read_error_detail();
    runtime().spawn(async move {
        match fut.await {
            Ok(detail) => ctx.ok(into_handle(ByteArray(detail)).cast()),
            Err(e) => ctx.err_of(&e),
        }
    });
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
pub unsafe extern "C" fn autd3_client_checker(client: *const ClientHandle) -> *mut CheckerHandle {
    let Some(client) = (unsafe { handle_ref(client) }) else {
        return std::ptr::null_mut();
    };

    into_handle(CheckerHandle(client.0.checker()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_checker_check(
    checker: *const CheckerHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(checker) = (unsafe { handle_ref(checker) }) else {
        ctx.err("null checker");
        return;
    };

    let fut = checker.0.check();
    runtime().spawn(async move {
        match fut.await {
            Ok(status) => {
                let status = LinkStatus {
                    devices: status.devices,
                    recoveries: status.recoveries,
                };
                ctx.ok(into_handle(status).cast());
            }
            Err(e) => ctx.err_of(&e),
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
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(client) = (unsafe { handle_ref(client) }) else {
        ctx.err("null client");
        return;
    };

    let fut = client.0.stop();
    runtime().spawn(async move {
        match fut.await {
            Ok(()) => ctx.ok(std::ptr::null_mut()),
            Err(e) => ctx.err_of(&e),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_close(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let Some(ctx) = CompletionCtx::new(cb, user_data) else {
        return;
    };
    let Some(client) = (unsafe { handle_ref(client) }) else {
        ctx.err("null client");
        return;
    };

    let fut = client.0.close();
    runtime().spawn(async move {
        match fut.await {
            Ok(()) => ctx.ok(std::ptr::null_mut()),
            Err(e) => ctx.err_of(&e),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_free(client: *mut ClientHandle) {
    unsafe { drop_handle(client) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_string_array_len(array: *const StringArray) -> usize {
    let Some(array) = (unsafe { handle_ref(array) }) else {
        return 0;
    };

    array.0.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_string_array_get(
    array: *const StringArray,
    index: usize,
) -> *const c_char {
    let Some(array) = (unsafe { handle_ref(array) }) else {
        return std::ptr::null();
    };

    array.0.get(index).map_or(std::ptr::null(), |s| s.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_string_array_free(array: *mut StringArray) {
    unsafe { drop_handle(array) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_recoveries(status: *const LinkStatus) -> u64 {
    let Some(status) = (unsafe { handle_ref(status) }) else {
        return 0;
    };

    status.recoveries
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_num_devices(status: *const LinkStatus) -> usize {
    let Some(status) = (unsafe { handle_ref(status) }) else {
        return 0;
    };

    status.devices.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_device_state(
    status: *const LinkStatus,
    index: usize,
    out_kind: *mut u8,
    out_bits: *mut u8,
) -> bool {
    let Some(status) = (unsafe { handle_ref(status) }) else {
        return false;
    };

    let Some(state) = status.devices.get(index) else {
        return false;
    };
    let (kind, bits) = match state {
        DeviceState::Op => (0, 0),
        DeviceState::SafeOp => (1, 0),
        DeviceState::SafeOpError => (2, 0),
        DeviceState::Lost => (3, 0),
        DeviceState::Other(bits) => (4, *bits),
    };

    unsafe { write_out(out_kind, kind) == AUTD3_OK && write_out(out_bits, bits) == AUTD3_OK }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_free(status: *mut LinkStatus) {
    unsafe { drop_handle(status) }
}

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;
    use autd3_ffi_abi::AUTD3_RT_PRIORITY_DEFAULT;

    #[test]
    fn a_new_client_config_matches_the_rust_default() {
        let handle = autd3_client_config_new();
        let config = unsafe { take_handle(handle) }.unwrap();
        let expected = ClientConfig::default();

        assert_eq!(expected.timeout_cycles, config.timeout_cycles);
        assert_eq!(expected.max_inflight, config.max_inflight);
        assert_eq!(expected.max_resync_rounds, config.max_resync_rounds);
        assert_eq!(expected.low_latency, config.low_latency);
        assert_eq!(expected.reset_resend_cycles, config.reset_resend_cycles);
        assert_eq!(expected.rt_priority, config.rt_priority);
        assert_eq!(expected.rt_policy, config.rt_policy);
        assert_eq!(expected.rt_affinity, config.rt_affinity);
        assert_eq!(expected.validate_state, config.validate_state);
        assert_eq!(
            expected.require_supported_firmware,
            config.require_supported_firmware
        );
    }

    #[test]
    fn the_default_rt_priority_mode_keeps_the_rust_default() {
        let handle = autd3_client_config_new();
        assert_eq!(AUTD3_OK, unsafe {
            autd3_client_config_set_rt_priority(handle, AUTD3_RT_PRIORITY_DEFAULT, 0)
        });
        let config = unsafe { take_handle(handle) }.unwrap();
        assert_eq!(ClientConfig::default().rt_priority, config.rt_priority);
        assert!(config.rt_priority.is_some());
    }

    #[test]
    fn an_unknown_rt_priority_mode_is_rejected() {
        let handle = autd3_client_config_new();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_config_set_rt_priority(handle, 9, 0)
        });
        unsafe { autd3_client_config_free(handle) };
    }

    #[test]
    fn a_zero_nonzero_setter_argument_is_rejected() {
        let handle = autd3_client_config_new();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_config_set_timeout_cycles(handle, 0)
        });
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_client_config_set_max_inflight(handle, 0)
        });
        unsafe { autd3_client_config_free(handle) };
    }

    #[test]
    fn unknown_enum_discriminants_are_rejected() {
        assert!(to_pattern_bank(2).is_none());
        assert!(to_modulation_bank(2).is_none());
        assert!(to_gpio_in(4).is_none());
        assert!(to_telemetry(0x07).is_none());
        assert!(to_pattern_stm_mode(3).is_none());
        assert!(to_pattern_compression(0).is_none());
        assert!(to_transition_mode(0x03, 0, 0).is_none());
        assert!(to_gpio_out(&Autd3GpioOut { kind: 14, value: 0 }).is_none());
    }

    #[test]
    fn every_telemetry_counter_round_trips() {
        assert_eq!(Some(Telemetry::SyncResync), to_telemetry(0x06));
        for counter in 0x00..=0x06u8 {
            assert!(to_telemetry(counter).is_some());
        }
    }

    #[test]
    fn a_non_representable_period_is_rejected_instead_of_panicking() {
        for secs in [-1.0, f32::NAN, f32::INFINITY, -f32::MAX, f32::MAX] {
            assert!(autd3_stm_config_period(secs).is_null());
            assert!(autd3_stm_config_period_nearest(secs).is_null());
        }

        let handle = autd3_stm_config_period(0.001);
        assert!(!handle.is_null());
        unsafe { autd3_stm_config_free(handle) };
    }

    #[test]
    fn a_size_wider_than_u32_is_rejected_instead_of_dividing_by_zero() {
        let handle = autd3_stm_config_period(1.0);
        let mut out = 0u16;
        assert_eq!(-1, unsafe {
            autd3_stm_config_into_sampling_config(handle, 1usize << 32, &raw mut out)
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

    fn one_device_geometry() -> *mut Geometry {
        into_handle(Geometry::new(vec![autd3_rs::Autd3::new(
            Point3::origin(),
            autd3_rs::UnitQuaternion::identity(),
        )]))
    }

    #[test]
    fn a_push_that_fails_leaves_the_op_handle_with_the_caller() {
        let op = autd3_op_clear();
        assert!(!op.is_null());

        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_datagram_builder_push(std::ptr::null_mut(), op)
        });

        let geometry = one_device_geometry();
        let builder = unsafe { autd3_datagram_builder_new(geometry) };
        assert_eq!(AUTD3_OK, unsafe {
            autd3_datagram_builder_push(builder, op)
        });

        unsafe { autd3_datagram_builder_free(builder) };
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_rejected_push_each_leaves_every_op_handle_with_the_caller() {
        let nested: *mut Pending = into_handle(Pending::Each(vec![None]));
        let ops = [nested];

        assert!(unsafe { take_each(ops.as_ptr(), ops.len()) }.is_none());

        let geometry = one_device_geometry();
        let builder = unsafe { autd3_datagram_builder_new(geometry) };
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_datagram_builder_push_each(builder, ops.as_ptr(), ops.len())
        });

        unsafe { autd3_op_free(nested) };
        unsafe { autd3_datagram_builder_free(builder) };
        unsafe { drop_handle(geometry) };
    }

    #[test]
    fn a_failed_open_leaves_the_link_handle_with_the_caller() {
        extern "C" fn never_reports_success(
            code: i32,
            _value: *mut c_void,
            _msg: *const c_char,
            _user_data: *mut c_void,
        ) {
            assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        }

        let opener: ClientOpener = Box::new(|_geometry, _config| unreachable!());
        let opener = into_handle(opener);
        let config = autd3_client_config_new();

        unsafe {
            autd3_client_open(
                std::ptr::null(),
                opener,
                config,
                Some(never_reports_success),
                std::ptr::null_mut(),
            );
        }

        assert!(unsafe { take_handle(opener) }.is_some());
        unsafe { autd3_client_config_free(config) };
    }

    #[test]
    fn a_null_completion_callback_is_ignored() {
        unsafe { autd3_client_close(std::ptr::null(), None, std::ptr::null_mut()) };
    }
}
