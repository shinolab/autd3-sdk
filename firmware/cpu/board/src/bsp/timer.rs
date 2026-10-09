use core::sync::atomic::{AtomicU32, Ordering};

use crate::bsp::vic;
use crate::regs::{
    CMT_CMSTR0, CMT_CMSTR0_STR0, CMT0_CMCNT, CMT0_CMCOR, CMT0_CMCR, CMT0_CMCR_CKS_MASK,
    CMT0_CMCR_CMIE, SYSTEM_MSTPCRA, VIC_INTNO_CMI0, modify16, modify32, read16, read32, write16,
};

const CMT0_PCLKD_HZ: u32 = 75_000_000;
const CMT0_CLOCK_DIVIDER: u32 = 8;
const CMT0_TICKS_PER_MS: u32 = (CMT0_PCLKD_HZ / CMT0_CLOCK_DIVIDER) / 1000;
const CMT0_COMPARE: u16 = 9374;
const _: () = assert!(CMT0_COMPARE as u32 + 1 == CMT0_TICKS_PER_MS);
const CMI0_PRIORITY: u32 = 15;

const MSTPCRA_CMT_UNIT0: u32 = 0x0000_0010;

static TICK_MS: AtomicU32 = AtomicU32::new(0);
static TICK_CONSUMED: AtomicU32 = AtomicU32::new(0);

pub(crate) fn init() {
    TICK_MS.store(0, Ordering::Relaxed);
    TICK_CONSUMED.store(0, Ordering::Relaxed);

    super::with_prcr(super::PRCR_LPC_UNLOCK, || {
        modify32(SYSTEM_MSTPCRA, |v| v & !MSTPCRA_CMT_UNIT0);
        let _ = read32(SYSTEM_MSTPCRA);
    });

    modify16(CMT_CMSTR0, |v| v & !CMT_CMSTR0_STR0);
    modify16(CMT0_CMCR, |v| (v & !CMT0_CMCR_CKS_MASK) | CMT0_CMCR_CMIE);
    write16(CMT0_CMCOR, CMT0_COMPARE);
    write16(CMT0_CMCNT, 0);
    vic::install_with(
        VIC_INTNO_CMI0,
        CMI0_PRIORITY,
        cmi0_entry as *const () as usize,
        true,
    );
    modify16(CMT_CMSTR0, |v| v | CMT_CMSTR0_STR0);
}

vic::irq_entry!(cmi0_entry, cmi0_isr);

extern "C" fn cmi0_isr() {
    vic::clear_edge(VIC_INTNO_CMI0);
    crate::udp::tick(TICK_MS.fetch_add(1, Ordering::Relaxed).wrapping_add(1));
    vic::end_of_interrupt();
}

pub(crate) fn now_ms() -> u32 {
    TICK_MS.load(Ordering::Relaxed)
}

pub(crate) fn elapsed_ms() -> u32 {
    let now = TICK_MS.load(Ordering::Relaxed);
    let consumed = TICK_CONSUMED.swap(now, Ordering::Relaxed);
    now.wrapping_sub(consumed)
}

pub(crate) fn delay_ms(ms: u16) {
    let mut remaining = u32::from(ms);
    let mut prev = read16(CMT0_CMCNT);
    while remaining != 0 {
        let now = read16(CMT0_CMCNT);
        if now < prev {
            remaining -= 1;
        }
        prev = now;
    }
}
