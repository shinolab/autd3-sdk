use std::ffi::{CString, c_char, c_void};
use std::num::NonZeroU16;
use std::sync::{Arc, OnceLock};

use autd3_ffi_abi::{
    ClientBackend, ClientOpener, CompletionCallback, CompletionCtx, DevicePattern,
    ModulationBuffer, PatternBuffer, drop_handle, into_handle,
};
use autd3_rs::value::{
    ModulationBank, PatternBank, PatternDataType, SamplingConfig, TransitionMode,
};
use autd3_rs::{
    ChangeModulationBank, ChangePatternBank, ClientConfig, ConfigModulation, ConfigPattern,
    DatagramBuilder as CoreDatagramBuilder, Datagrams, Geometry, Modulation, Pattern,
    WriteModulationBuffer, WritePatternBuffer,
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

fn to_transition_mode(v: u8) -> TransitionMode {
    match v {
        1 => TransitionMode::SysTime,
        2 => TransitionMode::Gpio,
        3 => TransitionMode::Ext,
        4 => TransitionMode::Immediate,
        _ => TransitionMode::SyncIdx,
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
        divider: u16,
        size: u32,
        data_type: PatternDataType,
    },
    ChangePatternBank {
        bank: PatternBank,
        transition_mode: TransitionMode,
        transition_value: u64,
    },
    WriteModulationBuffer {
        bank: ModulationBank,
        offset: u32,
        data: Vec<u8>,
    },
    ConfigModulation {
        bank: ModulationBank,
        divider: u16,
        size: u32,
    },
    ChangeModulationBank {
        bank: ModulationBank,
        transition_mode: TransitionMode,
        transition_value: u64,
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
pub extern "C" fn autd3_op_config_pattern(
    bank: u8,
    divider: u16,
    size: u32,
    data_type_kind: u8,
    num_foci: u8,
    sound_speed: u16,
) -> *mut Pending {
    into_handle(Pending::ConfigPattern {
        bank: to_pattern_bank(bank),
        divider,
        size,
        data_type: to_pattern_data_type(data_type_kind, num_foci, sound_speed),
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
        transition_mode: to_transition_mode(transition_mode),
        transition_value,
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
pub extern "C" fn autd3_op_config_modulation(bank: u8, divider: u16, size: u32) -> *mut Pending {
    into_handle(Pending::ConfigModulation {
        bank: to_modulation_bank(bank),
        divider,
        size,
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
        transition_mode: to_transition_mode(transition_mode),
        transition_value,
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
                divider,
                size,
                data_type,
            } => {
                core.push(ConfigPattern {
                    bank: *bank,
                    divider: *divider,
                    size: *size,
                    data_type: *data_type,
                });
            }
            Pending::ChangePatternBank {
                bank,
                transition_mode,
                transition_value,
            } => {
                core.push(ChangePatternBank {
                    bank: *bank,
                    transition_mode: *transition_mode,
                    transition_value: *transition_value,
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
                divider,
                size,
            } => {
                core.push(ConfigModulation {
                    bank: *bank,
                    divider: *divider,
                    size: *size,
                });
            }
            Pending::ChangeModulationBank {
                bank,
                transition_mode,
                transition_value,
            } => {
                core.push(ChangeModulationBank {
                    bank: *bank,
                    transition_mode: *transition_mode,
                    transition_value: *transition_value,
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
