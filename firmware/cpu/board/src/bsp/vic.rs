use core::arch::asm;

use crate::regs::{VIC_IEC, VIC_IEN0, VIC_PIC0, VIC_PLS0, modify32, vic_prl, vic_vad, write32};

pub(crate) fn init() {
    for reg in VIC_IEC {
        write32(reg, 0xFFFF_FFFF);
    }
}

pub(crate) fn irq_enable() {
    // SAFETY: `cpsie i` only unmasks IRQs on the current core. The application installs
    // its handlers before calling this, and the `memory` clobber keeps prior stores from
    // being sunk past the unmask.
    unsafe { asm!("cpsie i", options(nostack, preserves_flags)) };
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn install(intno: u32, priority: u32, handler: usize) {
    if intno == 0 || intno > 31 {
        loop {
            core::hint::spin_loop();
        }
    }

    write32(VIC_IEC[0], 1 << intno);
    modify32(VIC_PLS0, |v| v | (1 << intno));
    write32(vic_prl(intno), priority);
    write32(vic_vad(intno), handler as u32);
    write32(VIC_PIC0, 1 << intno);
    modify32(VIC_IEN0, |v| v | (1 << intno));
}
