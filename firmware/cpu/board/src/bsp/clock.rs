use core::arch::asm;

use crate::regs::{
    SYSTEM_LOCOCR, SYSTEM_LOCOCR_LCSTP, SYSTEM_PLL1CR, SYSTEM_PLL1CR2, SYSTEM_PRCR, SYSTEM_SCKCR,
    SYSTEM_SCKCR_CKIO_MASK, SYSTEM_SCKCR_CKIO_SHIFT, SYSTEM_SCKCR2, modify32, read32, write32,
};

const PRCR_CPG_UNLOCK: u32 = 0x0000_A501;
const PRCR_CPG_LOCK: u32 = 0x0000_A500;
const PLL1CR_CPUCKSEL_600_MHZ: u32 = 3;
const SCKCR_CKIO_75_MHZ: u32 = 0;
const PLL1_STABILIZE_LOOPS: u32 = 40000;

pub(crate) fn init() {
    write32(SYSTEM_PRCR, PRCR_CPG_UNLOCK);
    let _ = read32(SYSTEM_PRCR);

    modify32(SYSTEM_LOCOCR, |v| v & !SYSTEM_LOCOCR_LCSTP);

    write32(SYSTEM_PLL1CR, PLL1CR_CPUCKSEL_600_MHZ);
    let _ = read32(SYSTEM_PLL1CR);
    let _ = read32(SYSTEM_PLL1CR);
    let _ = read32(SYSTEM_PLL1CR);

    write32(SYSTEM_PLL1CR2, 1);
    for _ in 0..PLL1_STABILIZE_LOOPS {
        // SAFETY: `nop` has no operands and no memory effects; it only burns a cycle.
        unsafe { asm!("nop", options(nomem, nostack, preserves_flags)) };
    }

    write32(SYSTEM_SCKCR2, 1);

    modify32(SYSTEM_SCKCR, |v| {
        (v & !SYSTEM_SCKCR_CKIO_MASK) | (SCKCR_CKIO_75_MHZ << SYSTEM_SCKCR_CKIO_SHIFT)
    });

    write32(SYSTEM_PRCR, PRCR_CPG_LOCK);
    let _ = read32(SYSTEM_PRCR);
}
