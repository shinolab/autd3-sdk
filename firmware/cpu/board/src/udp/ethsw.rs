use autd3_cpu_fw::net::Mac;

use super::irq::without_irq;
use super::regs::{
    ATIME, ATIME_CORR, ATIME_CTRL, ATIME_EVT_PERIOD, ATIME_INC, ATIME_SEC, EMACRST, ETHPHYLNK,
    ETHSFTRST, ETHSWMD, ETHSWMTC, ETSPCMD, MACSEL, MII_CTRL0, MII_CTRL1, PORT0_CTRL, PORT1_CTRL,
    SPCMD, SW_ADR_TABLE, SW_ADR_TABLE_ENTRIES, SW_BCAST_DEFAULT_MASK, SW_HUB_CONTROL,
    SW_INPUT_LEARN_BLOCK, SW_MAC_BASE, SW_MAC_COMMAND_CONFIG, SW_MAC_FRM_LENGTH,
    SW_MAC_TX_SECTION_EMPTY, SW_MAC_TX_SECTION_FULL, SW_MCAST_DEFAULT_MASK, SW_MGMT_CONFIG,
    SW_OQMGR_STATUS, SW_PORT_ENA, SW_QMGR_ST_MINCELLS, SW_QMGR_WEIGHTS, SW_UCAST_DEFAULT_MASK,
    SWTMEN, SWTMLATNS, SWTMLATSEC, SWTMPNS, SWTMPSEC, SWTMSTNS, SWTMSTSEC, TSM_CONFIG,
    TSM_IRQ_STAT_ACK,
};
use crate::bsp::timer::delay_ms;
use crate::regs::{read32, write32};

const PROTECT_SEQUENCE: [u32; 4] = [0x00A5, 0x0001, 0xFFFE, 0x0001];

const MACSEL_SWITCH: u32 = 0;
const MII_CTRL_MII_FULL_DUPLEX: u32 = 0x0100;
const ETHPHYLNK_ALL_ACTIVE_LOW: u32 = 0x0F;
const ETHSWMTC_TAG_ENABLED: u32 = 0x8000_E001;
const ETHSFTRST_SWITCH_PHY_MII: u32 = (1 << 1) | (1 << 2) | (1 << 4);
const PHY_RESET_ASSERT_MS: u16 = 10;
const SWITCH_SETTLE_MS: u16 = 100;

const MASK_INTERNAL_ONLY: u32 = 0b100;
const MASK_EXTERNAL_ONLY: u32 = 0b011;
const MASK_ALL: u32 = 0b111;
const LEARNING_DISABLED_ALL: u32 = 0b111 << 16;
const MGMT_PORT_INTERNAL: u32 = 0b10;
const OQMGR_BUSYINIT: u32 = 1 << 0;
const OQMGR_INIT_MAX_POLLS: u32 = 1_000_000;
const QMGR_WEIGHTS_DEFAULT: u32 = 0x0804_0201;
const HUB_DISABLED: u32 = 0x0000_00A0;
const PORT_ENA_ALL: u32 = 0b111;
const MAC_COMMAND_CONFIG_ENABLE: u32 = 0x0580_0013;
const MAC_FRM_LENGTH: u32 = 0x0600;
const MAC_TX_SECTION_EMPTY: u32 = 0x48;
const MAC_TX_SECTION_FULL: u32 = 0x14;

const STATIC_ENTRY_VALID: u32 = 1 << 16;
const STATIC_ENTRY_STATIC: u32 = 1 << 17;
const STATIC_ENTRY_PORT_INTERNAL: u32 = 1 << 23;

const CLKPERD_NS: u32 = 10;
const ATIME_INC_10NS: u32 = (CLKPERD_NS << 8) | CLKPERD_NS;
const EVENT_PERIOD_NS: u32 = 1_000_000_000;
const ATIME_CTRL_ENABLE_WRAP: u32 = (1 << 0) | (1 << 5);
const ATIME_CTRL_CAPTURE: u32 = 1 << 11;
const CAPTURE_MAX_POLLS: u32 = 10_000;
const TSM_IRQ_CLEAR_ALL: u32 = 0x301F;

const SYNCOUT_START_DELAY_SEC: u64 = 2;
const NS_PER_SEC: u64 = 1_000_000_000;

fn unlock(reg: usize) {
    for value in PROTECT_SEQUENCE {
        write32(reg, value);
    }
}

fn lock(reg: usize) {
    write32(reg, 0);
}

pub(crate) fn select_switch() {
    unlock(SPCMD);
    write32(EMACRST, 0);
    lock(SPCMD);

    unlock(ETSPCMD);
    write32(ETHSFTRST, 0);
    let _ = read32(ETHSFTRST);
    lock(ETSPCMD);
    delay_ms(PHY_RESET_ASSERT_MS);
    unlock(ETSPCMD);
    write32(MACSEL, MACSEL_SWITCH);
    write32(MII_CTRL0, MII_CTRL_MII_FULL_DUPLEX);
    write32(MII_CTRL1, MII_CTRL_MII_FULL_DUPLEX);
    write32(ETHPHYLNK, ETHPHYLNK_ALL_ACTIVE_LOW);
    write32(ETHSWMTC, ETHSWMTC_TAG_ENABLED);
    write32(ETHSWMD, 0);
    write32(ETHSFTRST, ETHSFTRST_SWITCH_PHY_MII);
    lock(ETSPCMD);

    unlock(SPCMD);
    write32(EMACRST, 1);
    lock(SPCMD);

    delay_ms(SWITCH_SETTLE_MS);
}

pub(crate) fn init_switch() -> bool {
    for entry in 0..SW_ADR_TABLE_ENTRIES {
        write32(SW_ADR_TABLE + 8 * entry, 0);
        write32(SW_ADR_TABLE + 8 * entry + 4, 0);
    }
    set_forwarding(false);
    write32(SW_MGMT_CONFIG, MGMT_PORT_INTERNAL);

    let mut ok = false;
    for _ in 0..OQMGR_INIT_MAX_POLLS {
        if read32(SW_OQMGR_STATUS) & OQMGR_BUSYINIT == 0 {
            ok = true;
            break;
        }
    }
    write32(SW_OQMGR_STATUS, 0);
    write32(SW_QMGR_ST_MINCELLS, 0);
    write32(SW_QMGR_WEIGHTS, QMGR_WEIGHTS_DEFAULT);

    for base in SW_MAC_BASE {
        write32(base + SW_MAC_TX_SECTION_EMPTY, MAC_TX_SECTION_EMPTY);
        write32(base + SW_MAC_TX_SECTION_FULL, MAC_TX_SECTION_FULL);
        write32(base + SW_MAC_FRM_LENGTH, MAC_FRM_LENGTH);
        write32(base + SW_MAC_COMMAND_CONFIG, MAC_COMMAND_CONFIG_ENABLE);
    }

    write32(SW_HUB_CONTROL, HUB_DISABLED);
    write32(SW_PORT_ENA, PORT_ENA_ALL);
    ok
}

pub(crate) fn set_forwarding(open: bool) {
    let (unicast, group) = if open {
        (MASK_EXTERNAL_ONLY, MASK_ALL)
    } else {
        (MASK_INTERNAL_ONLY, MASK_INTERNAL_ONLY)
    };
    write32(SW_UCAST_DEFAULT_MASK, unicast);
    write32(SW_BCAST_DEFAULT_MASK, group);
    write32(SW_MCAST_DEFAULT_MASK, group);
    write32(SW_INPUT_LEARN_BLOCK, LEARNING_DISABLED_ALL);
}

pub(crate) fn set_own_mac(mac: Mac) {
    let low = u32::from_le_bytes([mac[0], mac[1], mac[2], mac[3]]);
    let high = u32::from(mac[4])
        | (u32::from(mac[5]) << 8)
        | STATIC_ENTRY_VALID
        | STATIC_ENTRY_STATIC
        | STATIC_ENTRY_PORT_INTERNAL;
    for entry in 0..SW_ADR_TABLE_ENTRIES {
        write32(SW_ADR_TABLE + 8 * entry, low);
        write32(SW_ADR_TABLE + 8 * entry + 4, high);
    }
}

pub(crate) fn init_timer() {
    write32(TSM_CONFIG, 0);
    write32(TSM_IRQ_STAT_ACK, TSM_IRQ_CLEAR_ALL);
    write32(ATIME_EVT_PERIOD, EVENT_PERIOD_NS);
    write32(ATIME_INC, ATIME_INC_10NS);
    write32(ATIME_CORR, 0);
    write32(ATIME_SEC, 0);
    write32(ATIME, 0);
    write32(ATIME_CTRL, ATIME_CTRL_ENABLE_WRAP);
    write32(PORT0_CTRL, 0);
    write32(PORT1_CTRL, 0);
}

fn capture_unguarded() -> Option<u64> {
    write32(ATIME_CTRL, ATIME_CTRL_CAPTURE);
    for _ in 0..CAPTURE_MAX_POLLS {
        if read32(ATIME_CTRL) & ATIME_CTRL_CAPTURE == 0 {
            let sec = read32(ATIME_SEC);
            let ns = read32(ATIME);
            return Some(u64::from(sec) * NS_PER_SEC + u64::from(ns));
        }
    }
    None
}

pub(crate) fn capture() -> Option<u64> {
    without_irq(capture_unguarded)
}

pub(crate) fn set_time(ns: u64) -> bool {
    let Ok(sec) = u32::try_from(ns / NS_PER_SEC) else {
        return false;
    };
    without_irq(|| {
        write32(ATIME_SEC, sec);
        write32(ATIME, (ns % NS_PER_SEC) as u32);
    });
    true
}

pub(crate) fn stop_syncout() {
    write32(SWTMEN, 0);
}

pub(crate) fn start_syncout(period_ns: u32) -> bool {
    stop_syncout();
    let Some(now) = capture() else {
        return false;
    };
    let Ok(start_sec) = u32::try_from(now / NS_PER_SEC + SYNCOUT_START_DELAY_SEC) else {
        return false;
    };
    write32(SWTMSTSEC, start_sec);
    write32(SWTMSTNS, 0);
    write32(SWTMPSEC, 0);
    write32(SWTMPNS, period_ns);
    write32(SWTMEN, 1);
    true
}

pub(crate) fn syncout_latch() -> u64 {
    u64::from(read32(SWTMLATSEC)) * NS_PER_SEC + u64::from(read32(SWTMLATNS))
}
