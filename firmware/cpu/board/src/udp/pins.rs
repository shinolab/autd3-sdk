use super::regs::{CS1BCR, CS1WCR};
use crate::bsp::{pfs_write_disable, pfs_write_enable};
use crate::regs::{
    PFS_BUS, PFS_BUS_ALT, PORT_2, PORT_4, PORT_5, PORT_8, PORT_B, PORT_C, PORT_D, PORT_F, PORT_J,
    PORT_U, SYSTEM_MSTPCRA, SYSTEM_MSTPCRB, SYSTEM_MSTPCRC, SYSTEM_SCKCR, modify8, modify32, pfs,
    pmr, read32, write8,
};

const PFS_ETH: u8 = 0x11;
const PFS_PHYRESETOUT: u8 = 0x16;

const ETH_PINS: [(usize, usize, u8); 41] = [
    (PORT_5, 0, PFS_ETH),
    (PORT_5, 1, PFS_ETH),
    (PORT_5, 2, PFS_ETH),
    (PORT_5, 3, PFS_ETH),
    (PORT_5, 4, PFS_ETH),
    (PORT_5, 6, PFS_ETH),
    (PORT_8, 0, PFS_ETH),
    (PORT_8, 1, PFS_ETH),
    (PORT_8, 2, PFS_ETH),
    (PORT_8, 3, PFS_ETH),
    (PORT_8, 4, PFS_ETH),
    (PORT_8, 5, PFS_ETH),
    (PORT_8, 6, PFS_ETH),
    (PORT_8, 7, PFS_ETH),
    (PORT_B, 0, PFS_ETH),
    (PORT_B, 1, PFS_ETH),
    (PORT_B, 2, PFS_ETH),
    (PORT_B, 3, PFS_ETH),
    (PORT_B, 4, PFS_ETH),
    (PORT_B, 5, PFS_ETH),
    (PORT_B, 6, PFS_ETH),
    (PORT_B, 7, PFS_ETH),
    (PORT_C, 0, PFS_ETH),
    (PORT_C, 1, PFS_ETH),
    (PORT_C, 2, PFS_ETH),
    (PORT_C, 3, PFS_ETH),
    (PORT_D, 5, PFS_ETH),
    (PORT_D, 6, PFS_ETH),
    (PORT_D, 7, PFS_ETH),
    (PORT_F, 5, PFS_ETH),
    (PORT_F, 6, PFS_ETH),
    (PORT_F, 7, PFS_ETH),
    (PORT_U, 6, PFS_PHYRESETOUT),
    (PORT_J, 0, PFS_ETH),
    (PORT_J, 1, PFS_ETH),
    (PORT_J, 2, PFS_ETH),
    (PORT_J, 3, PFS_ETH),
    (PORT_J, 4, PFS_ETH),
    (PORT_J, 5, PFS_ETH),
    (PORT_J, 6, PFS_ETH),
    (PORT_J, 7, PFS_ETH),
];

const BUS_PINS: [(usize, usize, u8); 4] = [
    (PORT_D, 1, PFS_BUS_ALT),
    (PORT_D, 2, PFS_BUS_ALT),
    (PORT_2, 2, PFS_BUS),
    (PORT_4, 5, PFS_BUS),
];

const PMR_SET: [(usize, u8); 10] = [
    (PORT_5, 0x5F),
    (PORT_8, 0xFF),
    (PORT_B, 0xFF),
    (PORT_C, 0x0F),
    (PORT_D, 0xE6),
    (PORT_F, 0xE0),
    (PORT_J, 0xFF),
    (PORT_U, 0x40),
    (PORT_2, 0x04),
    (PORT_4, 0x20),
];

const SCKCR_ETCKD_MASK: u32 = 0x0000_C000;
const SCKCR_ETCKD_1_563_MHZ: u32 = 3 << 14;
const SCKCR_ETCKE_MASK: u32 = 0x0000_1000;

const CS1BCR_MASK: u32 = 0x7FFF_7600;
const CS1BCR_VALUE: u32 = (1 << 28) | (1 << 25) | (1 << 19) | (2 << 9);
const CS1WCR_MASK: u32 = 0x0000_1FC3;
const CS1WCR_WR_SHIFT: u32 = 7;
const CS1WCR_WR_MASK: u32 = 0xF << CS1WCR_WR_SHIFT;
const CS1WCR_VALUE: u32 = (1 << 11) | (3 << CS1WCR_WR_SHIFT) | (1 << 6);

const MSTPCRA_PPG1: u32 = 1 << 5;
const MSTPCRA_MTU: u32 = 1 << 11;
const MSTPCRB_ETHERNET: u32 = 0x000F_C000;
const MSTPCRC_ELC: u32 = 1 << 6;

pub(crate) fn init() {
    modify32(SYSTEM_SCKCR, |v| {
        (v & !(SCKCR_ETCKD_MASK | SCKCR_ETCKE_MASK)) | SCKCR_ETCKD_1_563_MHZ
    });

    pfs_write_enable();
    for (port, pin, value) in ETH_PINS {
        write8(pfs(port, pin), value);
    }
    for (port, pin, value) in BUS_PINS {
        write8(pfs(port, pin), value);
    }
    pfs_write_disable();
    for (port, bits) in PMR_SET {
        modify8(pmr(port), |v| v | bits);
    }

    modify32(CS1BCR, |v| (v & !CS1BCR_MASK) | CS1BCR_VALUE);
    modify32(CS1WCR, |v| (v & !CS1WCR_MASK) | CS1WCR_VALUE);

    modify32(SYSTEM_MSTPCRB, |v| v & !MSTPCRB_ETHERNET);
    let _ = read32(SYSTEM_MSTPCRB);
    modify32(SYSTEM_MSTPCRA, |v| v & !(MSTPCRA_PPG1 | MSTPCRA_MTU));
    let _ = read32(SYSTEM_MSTPCRA);
    modify32(SYSTEM_MSTPCRC, |v| v & !MSTPCRC_ELC);
    let _ = read32(SYSTEM_MSTPCRC);
}

pub(crate) fn set_cs1_wait(cycles: u32) {
    modify32(CS1WCR, |v| {
        (v & !CS1WCR_WR_MASK) | ((cycles << CS1WCR_WR_SHIFT) & CS1WCR_WR_MASK)
    });
    let _ = read32(CS1WCR);
}
