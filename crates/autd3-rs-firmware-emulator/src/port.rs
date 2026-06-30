use core::cell::Cell;
use std::sync::Mutex;

use crate::fpga::FpgaEmulator;

pub(crate) static FW_LOCK: Mutex<()> = Mutex::new(());

thread_local! {


    static ACTIVE: Cell<*mut FpgaEmulator> = const { Cell::new(core::ptr::null_mut()) };
}

pub(crate) struct ActiveGuard;

impl ActiveGuard {
    pub(crate) fn set(fpga: *mut FpgaEmulator) -> Self {
        ACTIVE.with(|a| a.set(fpga));
        Self
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.set(core::ptr::null_mut()));
    }
}

fn with_active<R>(f: impl FnOnce(&mut FpgaEmulator) -> R) -> R {
    let p = ACTIVE.with(Cell::get);
    assert!(!p.is_null(), "no active FPGA emulator");

    unsafe { f(&mut *p) }
}

#[unsafe(no_mangle)]
extern "C" fn port_fpga_write(addr: u16, value: u16) {
    with_active(|fpga| fpga.write(addr, value));
}

#[unsafe(no_mangle)]
extern "C" fn port_fpga_read(addr: u16) -> u16 {
    with_active(|fpga| fpga.read(addr))
}

#[unsafe(no_mangle)]
extern "C" fn port_next_sync0() -> u64 {
    with_active(FpgaEmulator::next_sync0)
}

#[unsafe(no_mangle)]
extern "C" fn port_dc_sys_time() -> u64 {
    with_active(FpgaEmulator::dc_sys_time)
}

#[unsafe(no_mangle)]
extern "C" fn port_sleep_ms(_ms: u16) {}
