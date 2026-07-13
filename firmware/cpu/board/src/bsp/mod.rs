pub(crate) mod bus;
pub(crate) mod clock;
pub(crate) mod io;
pub(crate) mod timer;
pub(crate) mod vic;

use crate::regs::{MPC_PWPR, read8, write8};

const PWPR_PFSWE_ENABLE: u8 = 0x40;
const PWPR_PFSWE_CLEAR: u8 = 0x00;
const PWPR_B0WI: u8 = 0x80;

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
