use core::arch::asm;

use crate::regs::VIC_HVA0;
use crate::regs::{VIC_IEC, VIC_IEN0, VIC_PIC0, VIC_PLS0, modify32, vic_prl, vic_vad, write32};

const VECTORS_PER_BANK: u32 = 32;
const MAX_VECTOR: u32 = 255;

pub(crate) fn init() {
    for reg in VIC_IEC {
        write32(reg, 0xFFFF_FFFF);
    }
}

pub(crate) fn irq_enable() {
    unsafe { asm!("cpsie i", options(nostack, preserves_flags)) };
}

fn bank(intno: u32) -> usize {
    4 * (intno / VECTORS_PER_BANK) as usize
}

fn bit(intno: u32) -> u32 {
    1 << (intno % VECTORS_PER_BANK)
}

pub(crate) fn install(intno: u32, priority: u32, handler: usize) {
    install_with(intno, priority, handler, true);
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn install_with(intno: u32, priority: u32, handler: usize, edge: bool) {
    if intno == 0 || intno > MAX_VECTOR {
        loop {
            core::hint::spin_loop();
        }
    }

    write32(VIC_IEC[(intno / VECTORS_PER_BANK) as usize], bit(intno));
    modify32(VIC_PLS0 + bank(intno), |v| {
        if edge {
            v | bit(intno)
        } else {
            v & !bit(intno)
        }
    });
    write32(vic_prl(intno), priority);
    write32(vic_vad(intno), handler as u32);
    write32(VIC_PIC0 + bank(intno), bit(intno));
    modify32(VIC_IEN0 + bank(intno), |v| v | bit(intno));
}

pub(crate) fn clear_edge(intno: u32) {
    write32(VIC_PIC0 + bank(intno), bit(intno));
}

pub(crate) fn end_of_interrupt() {
    write32(VIC_HVA0, 0);
}
