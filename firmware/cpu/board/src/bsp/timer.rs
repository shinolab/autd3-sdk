use crate::regs::{
    CMT_CMSTR0, CMT_CMSTR0_STR0, CMT0_CMCNT, CMT0_CMCOR, CMT0_CMCR, CMT0_CMCR_CKS_MASK,
    CMT0_CMCR_CMIE, SYSTEM_MSTPCRA, SYSTEM_PRCR, modify16, modify32, read16, read32, write16,
    write32,
};

const CMT0_PCLKD_HZ: u32 = 75_000_000;
const CMT0_CLOCK_DIVIDER: u32 = 8;
const CMT0_TICKS_PER_MS: u32 = (CMT0_PCLKD_HZ / CMT0_CLOCK_DIVIDER) / 1000;

const MSTPCRA_CMT_UNIT0: u32 = 0x0000_0010;

const PRCR_LPC_UNLOCK: u32 = 0x0000_A502;
const PRCR_LPC_LOCK: u32 = 0x0000_A500;

pub(crate) fn init() {
    write32(SYSTEM_PRCR, PRCR_LPC_UNLOCK);
    let _ = read32(SYSTEM_PRCR);
    modify32(SYSTEM_MSTPCRA, |v| v & !MSTPCRA_CMT_UNIT0);
    let _ = read32(SYSTEM_MSTPCRA);
    write32(SYSTEM_PRCR, PRCR_LPC_LOCK);
    let _ = read32(SYSTEM_PRCR);

    modify16(CMT_CMSTR0, |v| v & !CMT_CMSTR0_STR0);
    modify16(CMT0_CMCR, |v| v & !(CMT0_CMCR_CKS_MASK | CMT0_CMCR_CMIE));
    write16(CMT0_CMCOR, 0xFFFF);
    write16(CMT0_CMCNT, 0);
    modify16(CMT_CMSTR0, |v| v | CMT_CMSTR0_STR0);
}

pub(crate) fn delay_ms(ms: u16) {
    let mut remaining = u32::from(ms) * CMT0_TICKS_PER_MS;
    let mut prev = read16(CMT0_CMCNT);

    while remaining != 0 {
        let now = read16(CMT0_CMCNT);
        let elapsed = u32::from(now.wrapping_sub(prev));
        prev = now;
        remaining = remaining.saturating_sub(elapsed);
    }
}
