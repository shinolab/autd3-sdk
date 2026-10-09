pub(crate) fn read8(addr: usize) -> u8 {
    unsafe { (addr as *const u8).read_volatile() }
}

pub(crate) fn write8(addr: usize, value: u8) {
    unsafe { (addr as *mut u8).write_volatile(value) }
}

pub(crate) fn read16(addr: usize) -> u16 {
    unsafe { (addr as *const u16).read_volatile() }
}

pub(crate) fn write16(addr: usize, value: u16) {
    unsafe { (addr as *mut u16).write_volatile(value) }
}

pub(crate) fn read32(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

pub(crate) fn write32(addr: usize, value: u32) {
    unsafe { (addr as *mut u32).write_volatile(value) }
}

pub(crate) fn dmb() {
    unsafe { core::arch::asm!("dmb", options(nostack, preserves_flags)) };
}

pub(crate) fn modify32(addr: usize, f: impl FnOnce(u32) -> u32) {
    write32(addr, f(read32(addr)));
}

pub(crate) fn modify16(addr: usize, f: impl FnOnce(u16) -> u16) {
    write16(addr, f(read16(addr)));
}

pub(crate) fn modify8(addr: usize, f: impl FnOnce(u8) -> u8) {
    write8(addr, f(read8(addr)));
}

pub(crate) const SYSTEM_SCKCR: usize = 0xA00B_0020;
pub(crate) const SYSTEM_SCKCR2: usize = 0xA00B_0024;
pub(crate) const SYSTEM_PLL1CR: usize = 0xA00B_0034;
pub(crate) const SYSTEM_PLL1CR2: usize = 0xA00B_0038;
pub(crate) const SYSTEM_LOCOCR: usize = 0xA00B_0040;
pub(crate) const SYSTEM_MSTPCRA: usize = 0xA00B_0300;
pub(crate) const SYSTEM_MSTPCRB: usize = 0xA00B_0304;
pub(crate) const SYSTEM_MSTPCRC: usize = 0xA00B_0308;
pub(crate) const SYSTEM_SWRR1: usize = 0xA00B_0210;
pub(crate) const SYSTEM_PRCR: usize = 0xA00B_0B00;

pub(crate) const SYSTEM_LOCOCR_LCSTP: u32 = 1;
pub(crate) const SYSTEM_SCKCR_CKIO_SHIFT: u32 = 8;
pub(crate) const SYSTEM_SCKCR_CKIO_MASK: u32 = 7 << SYSTEM_SCKCR_CKIO_SHIFT;

pub(crate) const VIC_IEN0: usize = 0xA001_0080;
pub(crate) const VIC_IEC: [usize; 10] = [
    0xA001_00A0,
    0xA001_00A4,
    0xA001_00A8,
    0xA001_00AC,
    0xA001_00B0,
    0xA001_00B4,
    0xA001_00B8,
    0xA001_00BC,
    0xA001_10A0,
    0xA001_10A4,
];
pub(crate) const VIC_PLS0: usize = 0xA001_0100;
pub(crate) const VIC_PIC0: usize = 0xA001_0120;
pub(crate) const VIC_HVA0: usize = 0xA001_0200;
pub(crate) const VIC_INTNO_CMI0: u32 = 21;

pub(crate) const fn vic_vad(n: u32) -> usize {
    0xA001_0400 + 4 * n as usize
}

pub(crate) const fn vic_prl(n: u32) -> usize {
    0xA001_0800 + 4 * n as usize
}

pub(crate) const CMT_CMSTR0: usize = 0xA008_0000;
pub(crate) const CMT0_CMCR: usize = 0xA008_0002;
pub(crate) const CMT0_CMCNT: usize = 0xA008_0004;
pub(crate) const CMT0_CMCOR: usize = 0xA008_0006;

pub(crate) const CMT_CMSTR0_STR0: u16 = 1;
pub(crate) const CMT0_CMCR_CKS_MASK: u16 = 3;
pub(crate) const CMT0_CMCR_CMIE: u16 = 1 << 6;

pub(crate) const PORT5_PDR: usize = 0xA000_000A;
pub(crate) const PORTA_PDR: usize = 0xA000_0014;
pub(crate) const PORTF_PDR: usize = 0xA000_001E;
pub(crate) const PORTN_PDR: usize = 0xA000_002C;
pub(crate) const PORTN_PODR: usize = 0xA000_0056;
pub(crate) const PORT1_DSCR: usize = 0xA000_0142;

pub(crate) fn pdr_set(reg: usize, pin: u16, val: u16) {
    modify16(reg, |v| (v & !(3 << (2 * pin))) | (val << (2 * pin)));
}

pub(crate) const MPC_PWPR: usize = 0xA000_02FF;

pub(crate) const PFS_BUS: u8 = 0x22;
pub(crate) const PFS_BUS_ALT: u8 = 0x23;

pub(crate) const fn pfs(port: usize, pin: usize) -> usize {
    0xA000_0200 + port * 8 + pin
}

pub(crate) const fn pmr(port: usize) -> usize {
    0xA000_0080 + port
}

pub(crate) const fn pfs_table<const N: usize>(pins: [(usize, usize, u8); N]) -> [(usize, u8); N] {
    let mut table = [(0, 0); N];
    let mut i = 0;
    while i < N {
        table[i] = (pfs(pins[i].0, pins[i].1), pins[i].2);
        i += 1;
    }
    table
}

pub(crate) const PORT_0: usize = 0;
pub(crate) const PORT_1: usize = 1;
pub(crate) const PORT_2: usize = 2;
pub(crate) const PORT_3: usize = 3;
pub(crate) const PORT_4: usize = 4;
pub(crate) const PORT_5: usize = 5;
pub(crate) const PORT_8: usize = 8;
pub(crate) const PORT_9: usize = 9;
pub(crate) const PORT_A: usize = 10;
pub(crate) const PORT_B: usize = 11;
pub(crate) const PORT_C: usize = 12;
pub(crate) const PORT_D: usize = 13;
pub(crate) const PORT_E: usize = 14;
pub(crate) const PORT_F: usize = 15;
pub(crate) const PORT_G: usize = 16;
pub(crate) const PORT_H: usize = 17;
pub(crate) const PORT_J: usize = 18;
pub(crate) const PORT_K: usize = 19;
pub(crate) const PORT_M: usize = 21;
pub(crate) const PORT_U: usize = 27;
