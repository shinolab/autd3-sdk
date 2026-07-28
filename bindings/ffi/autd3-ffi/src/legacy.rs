use std::cell::RefCell;
use std::ffi::{c_char, c_void};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

use autd3_ffi_abi::{
    CompletionCallback, CompletionCtx, LegacyClientBackend, LegacyClientOpener, drop_handle,
    into_handle,
};
use autd3_rs::Geometry;
use autd3_rs::Velocity;
use autd3_rs::commands::{
    ChangeModulationBank, Clear, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate,
    FociStmOption, ForceFan, Modulation, Nop, Pattern, PatternStm, PatternStmOption, SetGpioOut,
    SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, Synchronize,
};
use autd3_rs::legacy::{LegacyChangePatternBank, LegacyClientConfig, LegacyDatagramBuilder, LegacyFrames};
use autd3_rs::value::{PatternBank, SamplingConfig, TransitionMode};

use crate::{
    ByteArray, CheckerHandle, Pending, StringArray, runtime, to_cstrings, to_pattern_bank,
    to_transition_mode, write_cstr,
};

pub struct LegacyClientHandle(Box<dyn LegacyClientBackend>);

pub enum LegacyPending {
    LegacyChangePatternBank {
        kind: u8,
        bank: PatternBank,
        transition_mode: TransitionMode,
    },
}

enum LegacyItem {
    Current(Pending),
    Legacy(LegacyPending),
}

pub struct LegacyBuilder {
    geometry: Arc<Geometry>,
    pending: Vec<LegacyItem>,
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_legacy_client_config_new(timeout_cycles: u32) -> *mut LegacyClientConfig {
    let Some(timeout_cycles) = NonZeroU32::new(timeout_cycles) else {
        return std::ptr::null_mut();
    };
    into_handle(LegacyClientConfig { timeout_cycles })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_client_config_free(config: *mut LegacyClientConfig) {
    unsafe { drop_handle(config) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_datagram_builder_new(
    geometry: *const Geometry,
) -> *mut LegacyBuilder {
    if geometry.is_null() {
        return std::ptr::null_mut();
    }

    into_handle(LegacyBuilder {
        geometry: Arc::new(unsafe { &*geometry }.clone()),
        pending: Vec::new(),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_datagram_builder_push(
    builder: *mut LegacyBuilder,
    op: *mut Pending,
) {
    if builder.is_null() || op.is_null() {
        return;
    }

    let op = unsafe { *Box::from_raw(op) };

    unsafe { &mut *builder }
        .pending
        .push(LegacyItem::Current(op));
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_legacy_op_change_segment(
    kind: u8,
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
) -> *mut LegacyPending {
    into_handle(LegacyPending::LegacyChangePatternBank {
        kind,
        bank: to_pattern_bank(bank),
        transition_mode: to_transition_mode(transition_mode, transition_value, 0),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_op_free(op: *mut LegacyPending) {
    unsafe { drop_handle(op) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_datagram_builder_push_legacy(
    builder: *mut LegacyBuilder,
    op: *mut LegacyPending,
) {
    if builder.is_null() || op.is_null() {
        return;
    }

    let op = unsafe { *Box::from_raw(op) };

    unsafe { &mut *builder }
        .pending
        .push(LegacyItem::Legacy(op));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_datagram_builder_push_each(
    builder: *mut LegacyBuilder,
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
        .push(LegacyItem::Current(Pending::Each(devices)));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_datagram_builder_free(builder: *mut LegacyBuilder) {
    unsafe { drop_handle(builder) }
}

fn unsupported(name: &str) -> String {
    format!("legacy firmware does not support the `{name}` command")
}

type ErrSlot = Rc<RefCell<Option<String>>>;

struct PendingCommand<'a> {
    pending: &'a Pending,
    err: ErrSlot,
}

impl<'a> autd3_rs::legacy::LegacyCommand<'a> for PendingCommand<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        if let Err(e) = push_pending(self.pending, builder) {
            let mut slot = self.err.borrow_mut();
            if slot.is_none() {
                *slot = Some(e);
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn push_pending<'a>(
    pending: &'a Pending,
    builder: &mut LegacyDatagramBuilder<'a>,
) -> Result<(), String> {
    match pending {
        Pending::Pattern {
            emissions,
            bank,
            transition_mode,
        } => {
            builder.push(Pattern {
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
            let divider = std::num::NonZeroU16::new(*divider)
                .ok_or_else(|| "divider must be >= 1".to_owned())?;
            builder.push(Modulation {
                bank: *bank,
                config: SamplingConfig::new(divider),
                data,
                loop_behavior: *loop_behavior,
                transition_mode: *transition_mode,
            });
        }
        Pending::ChangeModulationBank {
            bank,
            transition_mode,
        } => {
            builder.push(ChangeModulationBank {
                bank: *bank,
                transition_mode: *transition_mode,
            });
        }
        Pending::Clear => {
            builder.push(Clear);
        }
        Pending::Synchronize => {
            builder.push(Synchronize);
        }
        Pending::Nop => {
            builder.push(Nop);
        }
        Pending::ForceFan(value) => {
            builder.push(ForceFan { value: *value });
        }
        Pending::SetSilencerCompletion {
            intensity,
            phase,
            strict,
        } => {
            builder.push(SetSilencer::new(FixedCompletionTime {
                intensity: *intensity,
                phase: *phase,
                strict_mode: *strict,
            }));
        }
        Pending::SetSilencerUpdateRate { intensity, phase } => {
            builder.push(SetSilencer::new(FixedUpdateRate {
                intensity: *intensity,
                phase: *phase,
            }));
        }
        Pending::SetSilencerDisable => {
            builder.push(SetSilencer::disable());
        }
        Pending::SetGpioOut(outputs) => {
            builder.push(SetGpioOut { outputs: *outputs });
        }
        Pending::EmulateGpioIn(values) => {
            builder.push(EmulateGpioIn { values: *values });
        }
        Pending::SetOutputMask(masks) => {
            builder.push(SetOutputMask { masks });
        }
        Pending::SetPhaseCorrection(phases) => {
            builder.push(SetPhaseCorrection { phases });
        }
        Pending::SetPulseWidthTable(t) => {
            builder.push(SetPulseWidthTable { table: t });
        }
        Pending::FociStm {
            config,
            points,
            bank,
            sound_speed,
            loop_behavior,
            transition_mode,
        } => {
            points.push_legacy_into(
                *config,
                FociStmOption {
                    bank: *bank,
                    sound_speed: Velocity::from_m_s(*sound_speed),
                    loop_behavior: *loop_behavior,
                    transition_mode: *transition_mode,
                },
                builder,
            );
        }
        Pending::PatternStm {
            config,
            patterns,
            bank,
            mode,
            loop_behavior,
            transition_mode,
        } => {
            builder.push(PatternStm::new(
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
        Pending::WritePatternBuffer { .. } => return Err(unsupported("WritePatternBuffer")),
        Pending::WriteFociBuffer { .. } => return Err(unsupported("WriteFociBuffer")),
        Pending::WritePatternCompressed { .. } => {
            return Err(unsupported("WritePatternCompressed"));
        }
        Pending::ConfigPattern { .. } => return Err(unsupported("ConfigPattern")),
        Pending::ConfigFociStm { .. } => return Err(unsupported("ConfigFociStm")),
        Pending::ChangePatternBank { .. } => return Err(unsupported("ChangePatternBank")),
        Pending::WriteModulationBuffer { .. } => return Err(unsupported("WriteModulationBuffer")),
        Pending::ConfigModulation { .. } => return Err(unsupported("ConfigModulation")),
        Pending::Each(devices) => {
            let err: ErrSlot = Rc::default();
            builder.push_each(|device| {
                devices
                    .get(device.idx())
                    .and_then(Option::as_ref)
                    .map(|pending| PendingCommand {
                        pending,
                        err: Rc::clone(&err),
                    })
            });
            if let Some(e) = err.borrow_mut().take() {
                return Err(e);
            }
        }
    }
    Ok(())
}

fn push_legacy_pending(
    pending: &LegacyPending,
    builder: &mut LegacyDatagramBuilder<'_>,
) -> Result<(), String> {
    match pending {
        LegacyPending::LegacyChangePatternBank {
            kind,
            bank,
            transition_mode,
        } => {
            builder.push(match kind {
                0 => LegacyChangePatternBank::pattern(*bank),
                1 => LegacyChangePatternBank::foci_stm(*bank, *transition_mode),
                2 => LegacyChangePatternBank::pattern_stm(*bank, *transition_mode),
                _ => return Err(format!("unknown change segment kind {kind}")),
            });
        }
    }
    Ok(())
}

fn push_item<'a>(
    item: &'a LegacyItem,
    builder: &mut LegacyDatagramBuilder<'a>,
) -> Result<(), String> {
    match item {
        LegacyItem::Current(pending) => push_pending(pending, builder),
        LegacyItem::Legacy(pending) => push_legacy_pending(pending, builder),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_datagram_builder_build(
    builder: *const LegacyBuilder,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut Arc<LegacyFrames> {
    if builder.is_null() {
        unsafe { write_cstr(out_err, out_err_len, "null builder") };
        return std::ptr::null_mut();
    }

    let builder = unsafe { &*builder };
    let mut legacy = LegacyDatagramBuilder::new(Arc::clone(&builder.geometry));
    for item in &builder.pending {
        if let Err(e) = push_item(item, &mut legacy) {
            unsafe { write_cstr(out_err, out_err_len, &e) };
            return std::ptr::null_mut();
        }
    }
    match legacy.build() {
        Ok(frames) => into_handle(Arc::new(frames)),
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_frames_num_frames(frames: *const Arc<LegacyFrames>) -> usize {
    if frames.is_null() {
        return 0;
    }

    unsafe { &*frames }.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_frames_free(frames: *mut Arc<LegacyFrames>) {
    unsafe { drop_handle(frames) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_client_open(
    geometry: *const Geometry,
    link: *mut LegacyClientOpener,
    config: *const LegacyClientConfig,
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
            Ok(backend) => ctx.ok(into_handle(LegacyClientHandle(backend)).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_client_num_devices(
    client: *const LegacyClientHandle,
) -> usize {
    if client.is_null() {
        return 0;
    }

    unsafe { &*client }.0.num_devices()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_client_send(
    client: *const LegacyClientHandle,
    frames: *const Arc<LegacyFrames>,
    frame: i64,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() || frames.is_null() {
        ctx.err("null argument");
        return;
    }

    let frames = unsafe { &*frames }.clone();
    let frame = usize::try_from(frame).ok();
    let fut = unsafe { &*client }.0.send(frames, frame);
    runtime().spawn(async move {
        match fut.await {
            Ok(data) => ctx.ok(into_handle(ByteArray(data)).cast()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_client_send_checked(
    client: *const LegacyClientHandle,
    frames: *const Arc<LegacyFrames>,
    frame: i64,
    cb: CompletionCallback,
    user_data: *mut c_void,
) {
    let ctx = CompletionCtx::new(cb, user_data);
    if client.is_null() || frames.is_null() {
        ctx.err("null argument");
        return;
    }

    let frames = unsafe { &*frames }.clone();
    let frame = usize::try_from(frame).ok();
    let fut = unsafe { &*client }.0.send_checked(frames, frame);
    runtime().spawn(async move {
        match fut.await {
            Ok(()) => ctx.ok(std::ptr::null_mut()),
            Err(e) => ctx.err(&e.to_string()),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_client_read_firmware_version(
    client: *const LegacyClientHandle,
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
pub unsafe extern "C" fn autd3_legacy_client_read_fpga_state(
    client: *const LegacyClientHandle,
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
pub unsafe extern "C" fn autd3_legacy_client_checker(
    client: *const LegacyClientHandle,
) -> *mut CheckerHandle {
    if client.is_null() {
        return std::ptr::null_mut();
    }

    into_handle(CheckerHandle(unsafe { &*client }.0.checker()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_legacy_client_stop(
    client: *const LegacyClientHandle,
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
pub unsafe extern "C" fn autd3_legacy_client_close(
    client: *const LegacyClientHandle,
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
pub unsafe extern "C" fn autd3_legacy_client_free(client: *mut LegacyClientHandle) {
    unsafe { drop_handle(client) }
}
