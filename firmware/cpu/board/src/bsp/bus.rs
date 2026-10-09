use crate::regs::{
    PFS_BUS, PFS_BUS_ALT, PORT_0, PORT_1, PORT_2, PORT_3, PORT_4, PORT_9, PORT_E, PORT_G, PORT_H,
    PORT_K, PORT1_DSCR, SYSTEM_MSTPCRC, pfs_table, pmr, read32, write8, write16, write32,
};

const MSTPCRC_RELEASE_BSC: u32 = 0x0000_7C7E;

const BUS_PINS: [(usize, u8); 40] = pfs_table([
    (PORT_0, 0, PFS_BUS),
    (PORT_0, 1, PFS_BUS),
    (PORT_0, 2, PFS_BUS),
    (PORT_0, 3, PFS_BUS),
    (PORT_0, 4, PFS_BUS),
    (PORT_0, 5, PFS_BUS),
    (PORT_0, 6, PFS_BUS),
    (PORT_0, 7, PFS_BUS),
    (PORT_1, 0, PFS_BUS),
    (PORT_1, 5, PFS_BUS),
    (PORT_2, 4, PFS_BUS),
    (PORT_3, 6, PFS_BUS),
    (PORT_3, 7, PFS_BUS),
    (PORT_4, 6, PFS_BUS),
    (PORT_9, 0, PFS_BUS_ALT),
    (PORT_E, 0, PFS_BUS),
    (PORT_E, 1, PFS_BUS),
    (PORT_E, 2, PFS_BUS),
    (PORT_E, 3, PFS_BUS),
    (PORT_E, 4, PFS_BUS),
    (PORT_E, 5, PFS_BUS),
    (PORT_E, 6, PFS_BUS),
    (PORT_E, 7, PFS_BUS),
    (PORT_G, 0, PFS_BUS),
    (PORT_G, 1, PFS_BUS),
    (PORT_G, 2, PFS_BUS),
    (PORT_G, 3, PFS_BUS),
    (PORT_G, 4, PFS_BUS),
    (PORT_G, 5, PFS_BUS),
    (PORT_G, 6, PFS_BUS),
    (PORT_G, 7, PFS_BUS),
    (PORT_H, 0, PFS_BUS),
    (PORT_H, 1, PFS_BUS),
    (PORT_H, 2, PFS_BUS),
    (PORT_H, 3, PFS_BUS),
    (PORT_H, 4, PFS_BUS),
    (PORT_H, 5, PFS_BUS),
    (PORT_H, 6, PFS_BUS),
    (PORT_H, 7, PFS_BUS),
    (PORT_K, 0, PFS_BUS_ALT),
]);

const PORT_MODES: [(usize, u8); 10] = [
    (pmr(PORT_0), 0xFF),
    (pmr(PORT_1), 0x21),
    (pmr(PORT_2), 0x10),
    (pmr(PORT_3), 0xD8),
    (pmr(PORT_4), 0x40),
    (pmr(PORT_9), 0x01),
    (pmr(PORT_E), 0xFF),
    (pmr(PORT_G), 0xFF),
    (pmr(PORT_H), 0xFF),
    (pmr(PORT_K), 0x01),
];

pub(crate) fn init() {
    super::pfs_write_enable();

    for (reg, value) in BUS_PINS {
        write8(reg, value);
    }

    super::pfs_write_disable();

    for (reg, value) in PORT_MODES {
        write8(reg, value);
    }

    write16(PORT1_DSCR, 0x0001);

    super::with_prcr(super::PRCR_LPC_UNLOCK, || {
        write32(SYSTEM_MSTPCRC, MSTPCRC_RELEASE_BSC);
        let _ = read32(SYSTEM_MSTPCRC);
    });
}
