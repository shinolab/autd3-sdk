use std::ffi::{CString, c_char, c_void};
use std::num::NonZeroU16;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use autd3_ffi_abi::{
    ClientBackend, ClientOpener, CompletionCallback, CompletionCtx, DevicePattern,
    ModulationBuffer, PatternBuffer, drop_handle, into_handle,
};
use autd3_rs::operation::Synchronize;
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::units::Hz;
use autd3_rs::value::{
    DcSysTime, Focus, GpioIn, Intensity, LoopBehavior, ModulationBank, PatternBank,
    PatternDataType, Phase, SamplingConfig, TransitionMode,
};
use autd3_rs::{
    ChangeModulationBank, ChangePatternBank, Clear, ClientConfig, ConfigModulation, ConfigPattern,
    ControlPoint, ControlPoints, DatagramBuilder as CoreDatagramBuilder, Datagrams, EmulateGpioIn,
    FixedCompletionTime, FixedUpdateRate, ForceFan, Geometry, GpioOut, Length, Modulation, Nop,
    PWE_TABLE_SIZE, Pattern, PatternStm, PatternStmMode, PatternStmOption, Point3, PulseWidth,
    SetGpioOut, SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, StmConfig,
    UnitVector3, Vector3, WriteFociBuffer, WriteModulationBuffer, WritePatternBuffer, circle, line,
};
use tokio::runtime::{Builder, Runtime};

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build autd3 ffi runtime")
    })
}

unsafe fn write_cstr(buf: *mut c_char, len: usize, s: &str) {
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

fn to_transition_mode(mode: u8, value: u64) -> TransitionMode {
    match mode {
        0x01 => TransitionMode::SysTime(DcSysTime::from_nanos(value)),
        #[allow(clippy::cast_possible_truncation)]
        0x02 => TransitionMode::Gpio(to_gpio_in(value as u8)),
        0xF0 => TransitionMode::Ext,
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
        10 => GpioOut::SysTimeEq(g.value),
        11 => GpioOut::SyncDiff,
        12 => GpioOut::PwmOut(g.value as u8),
        13 => GpioOut::Direct(g.value != 0),
        _ => GpioOut::None,
    }
}

fn to_pattern_data_type(kind: u8, num_foci: u8, sound_speed: u16) -> PatternDataType {
    if kind == 1 {
        PatternDataType::Foci {
            num_foci,
            sound_speed,
        }
    } else {
        PatternDataType::Raw
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

const FOCUS_UNIT_MM: f32 = 0.025;

#[allow(clippy::cast_possible_truncation)]
fn to_fixed(mm: f32) -> i32 {
    (mm / FOCUS_UNIT_MM).round() as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_freq(hz: f32) -> *mut StmConfig {
    into_handle(StmConfig::Freq(hz * Hz))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_freq_nearest(hz: f32) -> *mut StmConfig {
    into_handle(StmConfig::FreqNearest(hz * Hz))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_period(secs: f32) -> *mut StmConfig {
    into_handle(StmConfig::Period(Duration::from_secs_f32(secs)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_period_nearest(secs: f32) -> *mut StmConfig {
    into_handle(StmConfig::PeriodNearest(Duration::from_secs_f32(secs)))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_stm_config_sampling(divide: u16) -> *mut StmConfig {
    match NonZeroU16::new(divide) {
        Some(divide) => into_handle(StmConfig::Sampling(SamplingConfig::Divide(divide))),
        None => std::ptr::null_mut(),
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
    let points = circle(
        Point3::new(center[0], center[1], center[2]),
        Length::millimeters(radius_mm),
        num_points,
        UnitVector3::new_normalize(Vector3::new(normal[0], normal[1], normal[2])),
        Intensity(intensity),
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
    let points = line(
        Point3::new(start[0], start[1], start[2]),
        Point3::new(end[0], end[1], end[2]),
        num_points,
        Intensity(intensity),
    );
    unsafe { write_control_points(&points, out_points, out_intensities) };
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_client_config_new(low_latency: bool) -> *mut ClientConfig {
    into_handle(ClientConfig {
        low_latency,
        ..ClientConfig::default()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_client_config_free(config: *mut ClientConfig) {
    unsafe { drop_handle(config) }
}

pub enum Pending {
    Pattern(Vec<DevicePattern>),
    Modulation(u16, Vec<u8>),
    WritePatternBuffer {
        bank: PatternBank,
        index: u16,
        emissions: Vec<DevicePattern>,
    },
    ConfigPattern {
        bank: PatternBank,
        config: SamplingConfig,
        size: u32,
        data_type: PatternDataType,
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
    SetOutputMask(Vec<[bool; NUM_TRANSDUCERS]>),
    SetPhaseCorrection(Vec<[Phase; NUM_TRANSDUCERS]>),
    SetPulseWidthTable(Box<[u16; PWE_TABLE_SIZE]>),
    FociStm {
        config: StmConfig,
        samples: Vec<FociSample>,
        num_foci: u8,
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
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_pattern(pattern_buffer: *const PatternBuffer) -> *mut Pending {
    if pattern_buffer.is_null() {
        return std::ptr::null_mut();
    }

    into_handle(Pending::Pattern(unsafe { &*pattern_buffer }.0.clone()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_modulation(
    sampling_config: *const SamplingConfig,
    modulation_buffer: *const ModulationBuffer,
) -> *mut Pending {
    if sampling_config.is_null() || modulation_buffer.is_null() {
        return std::ptr::null_mut();
    }

    let Ok(divider) = unsafe { &*sampling_config }.divide() else {
        return std::ptr::null_mut();
    };
    let data = unsafe { &*modulation_buffer }.0.clone();
    into_handle(Pending::Modulation(divider, data))
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
pub unsafe extern "C" fn autd3_op_config_pattern(
    bank: u8,
    sampling_config: *const SamplingConfig,
    size: u32,
    data_type_kind: u8,
    num_foci: u8,
    sound_speed: u16,
    rep: u16,
) -> *mut Pending {
    if sampling_config.is_null() {
        return std::ptr::null_mut();
    }
    into_handle(Pending::ConfigPattern {
        bank: to_pattern_bank(bank),
        config: *unsafe { &*sampling_config },
        size,
        data_type: to_pattern_data_type(data_type_kind, num_foci, sound_speed),
        loop_behavior: rep_to_loop_behavior(rep),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_op_change_pattern_bank(
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
) -> *mut Pending {
    into_handle(Pending::ChangePatternBank {
        bank: to_pattern_bank(bank),
        transition_mode: to_transition_mode(transition_mode, transition_value),
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
) -> *mut Pending {
    into_handle(Pending::ChangeModulationBank {
        bank: to_modulation_bank(bank),
        transition_mode: to_transition_mode(transition_mode, transition_value),
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
    num_devices: usize,
) -> *mut Pending {
    if masks.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(masks, num_devices * NUM_TRANSDUCERS) };
    let masks = slice
        .chunks_exact(NUM_TRANSDUCERS)
        .map(|device| {
            let mut slot = [false; NUM_TRANSDUCERS];
            for (m, src) in slot.iter_mut().zip(device) {
                *m = *src != 0;
            }
            slot
        })
        .collect();
    into_handle(Pending::SetOutputMask(masks))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_set_phase_correction(
    phases: *const u8,
    num_devices: usize,
) -> *mut Pending {
    if phases.is_null() {
        return std::ptr::null_mut();
    }

    let slice = unsafe { std::slice::from_raw_parts(phases, num_devices * NUM_TRANSDUCERS) };
    let phases = slice
        .chunks_exact(NUM_TRANSDUCERS)
        .map(|device| {
            let mut slot = [Phase::ZERO; NUM_TRANSDUCERS];
            for (p, src) in slot.iter_mut().zip(device) {
                *p = Phase(*src);
            }
            slot
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
    let mut t = Box::new([0u16; PWE_TABLE_SIZE]);
    t.copy_from_slice(slice);
    into_handle(Pending::SetPulseWidthTable(t))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_pulse_width_default_table(out: *mut u16) {
    if out.is_null() {
        return;
    }

    let table = SetPulseWidthTable::default_table();
    unsafe { std::ptr::copy_nonoverlapping(table.as_ptr(), out, PWE_TABLE_SIZE) };
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
        .collect();
    into_handle(Pending::FociStm {
        config: *unsafe { &*config },
        samples,
        num_foci,
        bank: to_pattern_bank(bank),
        sound_speed: sound_speed_m_s,
        loop_behavior: rep_to_loop_behavior(loop_rep),
        transition_mode: to_transition_mode(transition_mode, transition_value),
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
        transition_mode: to_transition_mode(transition_mode, transition_value),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_op_free(op: *mut Pending) {
    unsafe { drop_handle(op) }
}

pub struct DatagramBuilder {
    num_devices: usize,
    pending: Vec<Pending>,
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_datagram_builder_new(num_devices: usize) -> *mut DatagramBuilder {
    into_handle(DatagramBuilder {
        num_devices,
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
pub unsafe extern "C" fn autd3_datagram_builder_free(builder: *mut DatagramBuilder) {
    unsafe { drop_handle(builder) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_lines)]
pub unsafe extern "C" fn autd3_datagram_builder_build(
    builder: *const DatagramBuilder,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut Arc<Datagrams> {
    if builder.is_null() {
        unsafe { write_cstr(out_err, out_err_len, "null builder") };
        return std::ptr::null_mut();
    }

    let builder = unsafe { &*builder };
    let mut core = CoreDatagramBuilder::new(builder.num_devices);
    for pending in &builder.pending {
        match pending {
            Pending::Pattern(emissions) => {
                core.push(Pattern::new(emissions));
            }
            Pending::Modulation(divider, data) => {
                let Some(divider) = NonZeroU16::new(*divider) else {
                    unsafe { write_cstr(out_err, out_err_len, "divider must be >= 1") };
                    return std::ptr::null_mut();
                };
                core.push(Modulation::new(SamplingConfig::Divide(divider), data));
            }
            Pending::WritePatternBuffer {
                bank,
                index,
                emissions,
            } => {
                core.push(WritePatternBuffer {
                    bank: *bank,
                    index: *index,
                    emissions,
                });
            }
            Pending::ConfigPattern {
                bank,
                config,
                size,
                data_type,
                loop_behavior,
            } => {
                core.push(ConfigPattern {
                    bank: *bank,
                    config: *config,
                    size: *size,
                    data_type: *data_type,
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
                    offset: *offset,
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
                    size: *size,
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
                samples,
                num_foci,
                bank,
                sound_speed,
                loop_behavior,
                transition_mode,
            } => {
                let n = samples.len();
                let sampling_config = config.into_sampling_config(n);
                let size = u32::try_from(n).unwrap_or(u32::MAX);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let sound_speed_value = (*sound_speed * 64.0).round() as u16;
                let mut foci = Vec::with_capacity(n * usize::from(*num_foci));
                for s in samples {
                    for (j, point) in s.points.iter().enumerate() {
                        let intensity_or_offset = if j == 0 {
                            s.intensity.0
                        } else {
                            point.phase_offset.0
                        };
                        foci.push(Focus {
                            x: to_fixed(point.point.x),
                            y: to_fixed(point.point.y),
                            z: to_fixed(point.point.z),
                            intensity_or_offset,
                        });
                    }
                }
                core.push(WriteFociBuffer {
                    bank: *bank,
                    offset: 0,
                    foci,
                })
                .push(ConfigPattern {
                    bank: *bank,
                    config: sampling_config,
                    size,
                    data_type: PatternDataType::Foci {
                        num_foci: *num_foci,
                        sound_speed: sound_speed_value,
                    },
                    loop_behavior: *loop_behavior,
                })
                .push(ChangePatternBank {
                    bank: *bank,
                    transition_mode: *transition_mode,
                });
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
pub unsafe extern "C" fn autd3_datagrams_num_frames(datagrams: *const Arc<Datagrams>) -> usize {
    if datagrams.is_null() {
        return 0;
    }

    unsafe { &*datagrams }.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_datagrams_free(datagrams: *mut Arc<Datagrams>) {
    unsafe { drop_handle(datagrams) }
}

pub struct ClientHandle(Box<dyn ClientBackend>);

pub struct StringArray(Vec<CString>);

pub struct ByteArray(Vec<u8>);

pub struct LinkStatus {
    device_states: Vec<CString>,
    all_op: bool,
    any_lost: bool,
    recoveries: u64,
}

fn to_cstrings(values: Vec<String>) -> Vec<CString> {
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
    datagrams: *const Arc<Datagrams>,
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
pub unsafe extern "C" fn autd3_client_check_status(
    client: *const ClientHandle,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() {
        ctx.err("null client");
        return;
    }

    let fut = unsafe { &*client }.0.check_status();
    runtime().spawn(async move {
        match fut.await {
            Ok(status) => {
                let status = LinkStatus {
                    device_states: to_cstrings(status.device_states),
                    all_op: status.all_op,
                    any_lost: status.any_lost,
                    recoveries: status.recoveries,
                };
                ctx.ok(into_handle(status).cast());
            }
            Err(e) => ctx.err(&e.to_string()),
        }
    });
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
pub unsafe extern "C" fn autd3_link_status_all_op(status: *const LinkStatus) -> bool {
    !status.is_null() && unsafe { &*status }.all_op
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_any_lost(status: *const LinkStatus) -> bool {
    !status.is_null() && unsafe { &*status }.any_lost
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

    unsafe { &*status }.device_states.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_device_state(
    status: *const LinkStatus,
    index: usize,
) -> *const c_char {
    if status.is_null() {
        return std::ptr::null();
    }

    unsafe { &*status }
        .device_states
        .get(index)
        .map_or(std::ptr::null(), |s| s.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_status_free(status: *mut LinkStatus) {
    unsafe { drop_handle(status) }
}
