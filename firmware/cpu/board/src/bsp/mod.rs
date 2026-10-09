pub(crate) mod bus;
pub(crate) mod clock;
pub(crate) mod io;
pub(crate) mod timer;
pub(crate) mod vic;

use crate::regs::{MPC_PWPR, SYSTEM_PRCR, read8, read32, write8, write32};

const PWPR_PFSWE_ENABLE: u8 = 0x40;
const PWPR_PFSWE_CLEAR: u8 = 0x00;
const PWPR_B0WI: u8 = 0x80;

pub(crate) const PRCR_CPG_UNLOCK: u32 = 0x0000_A501;
pub(crate) const PRCR_LPC_UNLOCK: u32 = 0x0000_A502;
const PRCR_LOCK: u32 = 0x0000_A500;

pub(crate) fn with_prcr(unlock_key: u32, f: impl FnOnce()) {
    write32(SYSTEM_PRCR, unlock_key);
    let _ = read32(SYSTEM_PRCR);
    f();
    write32(SYSTEM_PRCR, PRCR_LOCK);
    let _ = read32(SYSTEM_PRCR);
}

pub(crate) fn pfs_write_enable() {
    write8(MPC_PWPR, PWPR_PFSWE_CLEAR);
    let _ = read8(MPC_PWPR);
    write8(MPC_PWPR, PWPR_PFSWE_ENABLE);
    let _ = read8(MPC_PWPR);
}

pub(crate) fn pfs_write_disable() {
    write8(MPC_PWPR, PWPR_B0WI);
    let _ = read8(MPC_PWPR);
}
