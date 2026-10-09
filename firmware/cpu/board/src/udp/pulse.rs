use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use autd3_cpu_fw::udp::SYNC_CYCLE_NS;

use super::ethsw;
use super::regs::VIC_INTNO_TGIA0;
use super::regs::{
    ELC_ELCR, ELC_ELOPA, ELC_ELSR0, MTU_TSTRA, MTU0_TCNT, MTU0_TCR, MTU0_TCR2, MTU0_TGRA,
    MTU0_TIER, MTU0_TIORH, MTU0_TMDR1, PPG1_NDERL, PPG1_NDRL, PPG1_PCR, PPG1_PMR, PPG1_PODRL,
    PPG1_PTRSLR,
};
use crate::bsp::{pfs_write_disable, pfs_write_enable, vic};
use crate::regs::{PORT_M, modify8, pfs, pmr, write8, write16};

pub(crate) const HALF_PERIOD_NS: u32 = SYNC_CYCLE_NS / 2;

const PM3: usize = 3;
const PFS_PO16: u8 = 0x06;

const TSTRA_CST0: u8 = 1 << 0;
const TCR_PCLKC_DIV1: u8 = 0b000;
const TIORH_IOA_CAPTURE_RISING: u8 = 0b1000;
const TIER_TGIEA: u8 = 1 << 0;

const ELS_ETHSW_SYNCOUT: u8 = 0x22;
const ELOPA_MTU0_CAPTURE: u8 = 0b1111_1110;
const ELOPA_DISABLED: u8 = 0xFF;
const ELCR_ENABLE: u8 = 0xFF;

const PTRSL_MTU: u8 = 0;
const PCR_GROUP4_MTU0: u8 = 0;
const PMR_GROUP4_DIRECT: u8 = 1 << 4;
const PO16: u8 = 1 << 0;

static ARMED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static SERVED_LATCH: AtomicU64 = AtomicU64::new(u64::MAX);

pub(crate) fn is_ready() -> bool {
    READY.load(Ordering::Acquire)
}

fn setup_ppg() {
    write8(PPG1_NDERL, 0);
    write8(PPG1_PODRL, 0);
    write8(PPG1_PTRSLR, PTRSL_MTU);
    write8(PPG1_PCR, PCR_GROUP4_MTU0);
    write8(PPG1_PMR, PMR_GROUP4_DIRECT);
    write8(PPG1_NDRL, PO16);
    write8(PPG1_NDERL, PO16);

    pfs_write_enable();
    write8(pfs(PORT_M, PM3), PFS_PO16);
    pfs_write_disable();
    modify8(pmr(PORT_M), |v| v | (1 << PM3));
}

fn stop_capture() {
    write8(ELC_ELOPA, ELOPA_DISABLED);
    write8(ELC_ELSR0, 0);
    write8(MTU0_TIER, 0);
    modify8(MTU_TSTRA, |v| v & !TSTRA_CST0);
}

pub(crate) fn stop() {
    ARMED.store(false, Ordering::Release);
    READY.store(false, Ordering::Release);
    stop_capture();
    ethsw::stop_syncout();
}

pub(crate) fn arm() -> bool {
    stop();
    setup_ppg();
    write8(MTU0_TCR, TCR_PCLKC_DIV1);
    write8(MTU0_TCR2, 0);
    write8(MTU0_TMDR1, 0);
    write8(MTU0_TIORH, TIORH_IOA_CAPTURE_RISING);
    write16(MTU0_TCNT, 0);
    write16(MTU0_TGRA, 0);
    write8(MTU0_TIER, TIER_TGIEA);
    modify8(MTU_TSTRA, |v| v | TSTRA_CST0);
    write8(ELC_ELSR0, ELS_ETHSW_SYNCOUT);
    write8(ELC_ELOPA, ELOPA_MTU0_CAPTURE);
    write8(ELC_ELCR, ELCR_ENABLE);
    vic::clear_edge(VIC_INTNO_TGIA0);
    SERVED_LATCH.store(ethsw::syncout_latch(), Ordering::Relaxed);
    let started = ethsw::start_syncout(HALF_PERIOD_NS);
    if started {
        ARMED.store(true, Ordering::Release);
    }
    started
}

pub(crate) fn next_level_is_high(latch_ns: u64) -> bool {
    let period = u64::from(HALF_PERIOD_NS);
    let event = (latch_ns + period / 2) / period;
    (event + 1).is_multiple_of(2)
}

pub(crate) fn on_capture() {
    let latch = ethsw::syncout_latch();
    let high = next_level_is_high(latch);
    write8(PPG1_NDRL, if high { PO16 } else { 0 });
    SERVED_LATCH.store(latch, Ordering::Relaxed);
    if ARMED.load(Ordering::Acquire) {
        READY.store(true, Ordering::Release);
    }
}

pub(crate) fn service() {
    if ARMED.load(Ordering::Acquire)
        && ethsw::syncout_latch() != SERVED_LATCH.load(Ordering::Relaxed)
    {
        on_capture();
    }
}
