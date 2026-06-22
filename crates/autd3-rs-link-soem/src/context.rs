use std::cell::UnsafeCell;
use std::ffi::{CStr, CString, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::bindings::{
    ECT_REG_DCSYSDIFF, ECT_REG_DCSYSTIME, ec_ALstatuscode2string, ec_slavet, ecx_FPRD, ecx_FRMW,
    ecx_close, ecx_config_init, ecx_config_map_group, ecx_configdc, ecx_contextt, ecx_dcsync0,
    ecx_init, ecx_readstate, ecx_receive_processdata, ecx_send_processdata, ecx_statecheck,
    ecx_writestate,
};
use crate::error::SoemLinkError;
use crate::state::AlState;

#[allow(clippy::cast_possible_wrap)]
pub(crate) const EC_TIMEOUTRET_US: i32 = crate::bindings::EC_TIMEOUTRET as i32;

struct Sync0Config {
    cycle_ns: u32,
    shift_ns: i32,
}

unsafe extern "C" fn po2so_config(context: *mut ecx_contextt, slave: u16) -> i32 {
    // SAFETY: `userdata` points at the `Sync0Config` boxed by the owning
    // [`Context`], which outlives every FFI call made through it.
    unsafe {
        let sync0 = &*(*context).userdata.cast::<Sync0Config>();
        ecx_dcsync0(context, slave, 1, sync0.cycle_ns, sync0.shift_ns);
    }
    0
}

pub(crate) struct Context {
    ctx: UnsafeCell<ecx_contextt>,
    initialized: AtomicBool,
    _sync0: Box<Sync0Config>,
}

// SAFETY: the raw pointers inside `ecx_contextt` are only dereferenced by
// SOEM itself, which guards shared wire/index state with the port mutexes;
// the fields read directly from Rust (slavelist, grouplist, DCtime) are
// plain data updated by those same serialized calls.
unsafe impl Send for Context {}
// SAFETY: see `Send`; concurrent `&self` FFI calls are serialized by SOEM's
// port mutexes.
unsafe impl Sync for Context {}

impl Context {
    pub(crate) fn new(sync0_period: Duration, sync0_shift: Duration) -> Self {
        let sync0 = Box::new(Sync0Config {
            #[allow(clippy::cast_possible_truncation)]
            cycle_ns: sync0_period.as_nanos() as u32,
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            shift_ns: sync0_shift.as_nanos() as i32,
        });
        // SAFETY: `ecx_contextt` is a C struct that SOEM expects zeroed
        // before `ecx_init`; all-zero is a valid bit pattern for it
        // (`Option<fn>` fields become `None`).
        let mut ctx: ecx_contextt = unsafe { std::mem::zeroed() };
        ctx.userdata = std::ptr::from_ref::<Sync0Config>(&*sync0)
            .cast_mut()
            .cast::<c_void>();
        Self {
            ctx: UnsafeCell::new(ctx),
            initialized: AtomicBool::new(false),
            _sync0: sync0,
        }
    }

    fn as_ptr(&self) -> *mut ecx_contextt {
        self.ctx.get()
    }

    fn slave(&self, index: usize) -> *mut ec_slavet {
        // SAFETY: `index` is bounded by `num_slaves` (callers iterate
        // `0..num_slaves()`), and `slavelist` holds `EC_MAXSLAVE` entries.
        unsafe { &raw mut (*self.as_ptr()).slavelist[index + 1] }
    }

    pub(crate) fn init(&self, interface: &str) -> Result<(), SoemLinkError> {
        let ifname = CString::new(interface)
            .map_err(|_| SoemLinkError::InvalidInterfaceName(interface.to_string()))?;
        // SAFETY: `ifname` is a valid NUL-terminated string and the context
        // is zero-initialized as SOEM requires.
        if unsafe { ecx_init(self.as_ptr(), ifname.as_ptr()) } > 0 {
            self.initialized.store(true, Ordering::Release);
            Ok(())
        } else {
            Err(SoemLinkError::NoSocketConnection(interface.to_string()))
        }
    }

    pub(crate) fn config_init(&self) -> usize {
        // SAFETY: the context has been initialized by `init`.
        let wkc = unsafe { ecx_config_init(self.as_ptr()) };
        usize::try_from(wkc).unwrap_or(0)
    }

    pub(crate) fn num_slaves(&self) -> usize {
        // SAFETY: `slavecount` is plain data owned by the context.
        usize::try_from(unsafe { (*self.as_ptr()).slavecount }).unwrap_or(0)
    }

    pub(crate) fn slave_name(&self, index: usize) -> String {
        // SAFETY: SOEM NUL-terminates `name` during `config_init`.
        unsafe { CStr::from_ptr((*self.slave(index)).name.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    pub(crate) fn set_po2so_hooks(&self) {
        for index in 0..self.num_slaves() {
            // SAFETY: exclusive setup-phase write to this slave's hook field.
            unsafe { (*self.slave(index)).PO2SOconfig = Some(po2so_config) };
        }
    }

    pub(crate) fn configdc(&self) -> bool {
        // SAFETY: the context has been initialized and slaves enumerated.
        unsafe { ecx_configdc(self.as_ptr()) != 0 }
    }

    /// # Safety
    ///
    /// `iomap` must stay valid (and not move) for as long as process data is
    /// exchanged through this context; SOEM keeps pointers into it.
    pub(crate) unsafe fn config_map_group(&self, iomap: *mut c_void) -> usize {
        // SAFETY: caller guarantees `iomap` validity; group 0 always exists.
        let size = unsafe { ecx_config_map_group(self.as_ptr(), iomap, 0) };
        usize::try_from(size).unwrap_or(0)
    }

    pub(crate) fn statecheck(&self, slave: u16, request: AlState, timeout_us: i32) -> AlState {
        // SAFETY: plain FFI call on an initialized context.
        AlState(unsafe { ecx_statecheck(self.as_ptr(), slave, request.0, timeout_us) })
    }

    pub(crate) fn read_state(&self) {
        // SAFETY: plain FFI call on an initialized context.
        unsafe { ecx_readstate(self.as_ptr()) };
    }

    pub(crate) fn slave_state(&self, index: usize) -> AlState {
        // SAFETY: `state` is plain data updated by serialized SOEM calls.
        AlState(unsafe { (*self.slave(index)).state })
    }

    pub(crate) fn al_status_code(&self, index: usize) -> u16 {
        // SAFETY: `ALstatuscode` is plain data, see `slave_state`.
        unsafe { (*self.slave(index)).ALstatuscode }
    }

    pub(crate) fn al_status_string(&self, index: usize) -> String {
        let code = self.al_status_code(index);
        // SAFETY: `ec_ALstatuscode2string` returns a pointer to a static
        // NUL-terminated string table entry.
        let text = unsafe { CStr::from_ptr(ec_ALstatuscode2string(code)) }.to_string_lossy();
        format!("{text} ({code:#06x})")
    }

    pub(crate) fn request_state(&self, index: Option<usize>, state: AlState) {
        let slot = index.map_or(0, |i| i + 1);
        // SAFETY: slot 0 is the broadcast pseudo-slave; others are bounded
        // like `slave`.
        unsafe {
            (*self.as_ptr()).slavelist[slot].state = state.0;
            #[allow(clippy::cast_possible_truncation)]
            ecx_writestate(self.as_ptr(), slot as u16);
        }
    }

    pub(crate) fn send_processdata(&self) {
        // SAFETY: process data has been mapped by `config_map_group`.
        unsafe { ecx_send_processdata(self.as_ptr()) };
    }

    pub(crate) fn receive_processdata(&self, timeout_us: i32) -> i32 {
        // SAFETY: see `send_processdata`.
        unsafe { ecx_receive_processdata(self.as_ptr(), timeout_us) }
    }

    pub(crate) fn dc_time(&self) -> i64 {
        // SAFETY: `DCtime` is plain data updated by `receive_processdata`.
        unsafe { (*self.as_ptr()).DCtime }
    }

    pub(crate) fn expected_wkc(&self) -> i32 {
        // SAFETY: `grouplist[0]` is plain data set by `config_map_group`.
        let group = unsafe { &(*self.as_ptr()).grouplist[0] };
        i32::from(group.outputsWKC) * 2 + i32::from(group.inputsWKC)
    }

    pub(crate) fn copy_outputs(&self, index: usize, frame: &[u8]) {
        // SAFETY: `outputs` points into the caller-owned I/O map for
        // `Obytes` bytes; only this RT-thread method writes that region.
        unsafe {
            let slave = self.slave(index);
            let len = frame.len().min((*slave).Obytes as usize);
            std::ptr::copy_nonoverlapping(frame.as_ptr(), (*slave).outputs, len);
        }
    }

    pub(crate) fn copy_inputs(&self, index: usize, frame: &mut [u8]) {
        // SAFETY: `inputs` points into the caller-owned I/O map for
        // `Ibytes` bytes, filled by `receive_processdata`.
        unsafe {
            let slave = self.slave(index);
            let len = frame.len().min((*slave).Ibytes as usize);
            std::ptr::copy_nonoverlapping((*slave).inputs, frame.as_mut_ptr(), len);
        }
    }

    pub(crate) fn distribute_dc_time(&self) {
        let mut dc_time: u64 = 0;
        // SAFETY: slave 1 (the DC reference) exists once `config_init`
        // found at least one slave; `dc_time` outlives the call.
        unsafe {
            let configadr = (*self.slave(0)).configadr;
            #[allow(clippy::cast_possible_truncation)]
            ecx_FRMW(
                &raw mut (*self.as_ptr()).port,
                configadr,
                ECT_REG_DCSYSTIME as u16,
                std::mem::size_of::<u64>() as u16,
                (&raw mut dc_time).cast::<c_void>(),
                EC_TIMEOUTRET_US,
            );
        }
    }

    pub(crate) fn dc_system_time_difference(&self, index: usize) -> Option<u32> {
        let mut diff: u32 = 0;
        // SAFETY: `index` is bounded like `slave`; `diff` outlives the call.
        let wkc = unsafe {
            let configadr = (*self.slave(index)).configadr;
            #[allow(clippy::cast_possible_truncation)]
            ecx_FPRD(
                &raw mut (*self.as_ptr()).port,
                configadr,
                ECT_REG_DCSYSDIFF as u16,
                std::mem::size_of::<u32>() as u16,
                (&raw mut diff).cast::<c_void>(),
                EC_TIMEOUTRET_US,
            )
        };
        (wkc == 1).then_some(diff)
    }

    pub(crate) fn dcsync0_off(&self, index: usize) {
        // SAFETY: deactivation; cycle/shift are ignored when `act` is 0.
        unsafe {
            #[allow(clippy::cast_possible_truncation)]
            ecx_dcsync0(self.as_ptr(), (index + 1) as u16, 0, 0, 0);
        }
    }

    pub(crate) fn close(&self) {
        if self.initialized.swap(false, Ordering::AcqRel) {
            // SAFETY: runs at most once after a successful `init`; the
            // owning `Arc` guarantees no other thread is inside an FFI call.
            unsafe { ecx_close(self.as_ptr()) };
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        self.close();
    }
}
