pub(crate) fn read8(addr: usize) -> u8 {
    // SAFETY: `addr` is one of the fixed RZ/T1 MMIO addresses defined in this module.
    // They are always mapped, byte-accessible and have no Rust object aliasing them.
    unsafe { (addr as *const u8).read_volatile() }
}

pub(crate) fn write8(addr: usize, value: u8) {
    // SAFETY: see `read8`.
    unsafe { (addr as *mut u8).write_volatile(value) }
}

pub(crate) fn read16(addr: usize) -> u16 {
    // SAFETY: see `read8`; these registers are 16-bit accessible and 2-byte aligned.
    unsafe { (addr as *const u16).read_volatile() }
}

pub(crate) fn write16(addr: usize, value: u16) {
    // SAFETY: see `read16`.
    unsafe { (addr as *mut u16).write_volatile(value) }
}

pub(crate) fn read32(addr: usize) -> u32 {
    // SAFETY: see `read8`; these registers are 32-bit accessible and 4-byte aligned.
    unsafe { (addr as *const u32).read_volatile() }
}

pub(crate) fn write32(addr: usize, value: u32) {
    // SAFETY: see `read32`.
    unsafe { (addr as *mut u32).write_volatile(value) }
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
pub(crate) const SYSTEM_MSTPCRC: usize = 0xA00B_0308;
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
pub(crate) const PORT0_PMR: usize = 0xA000_0080;
pub(crate) const PORT1_PMR: usize = 0xA000_0081;
pub(crate) const PORT2_PMR: usize = 0xA000_0082;
pub(crate) const PORT3_PMR: usize = 0xA000_0083;
pub(crate) const PORT4_PMR: usize = 0xA000_0084;
pub(crate) const PORT9_PMR: usize = 0xA000_0089;
pub(crate) const PORTA_PMR: usize = 0xA000_008A;
pub(crate) const PORTE_PMR: usize = 0xA000_008E;
pub(crate) const PORTG_PMR: usize = 0xA000_0090;
pub(crate) const PORTH_PMR: usize = 0xA000_0091;
pub(crate) const PORTK_PMR: usize = 0xA000_0093;
pub(crate) const PORT1_DSCR: usize = 0xA000_0142;

pub(crate) fn pdr_set(reg: usize, pin: u16, val: u16) {
    modify16(reg, |v| (v & !(3 << (2 * pin))) | (val << (2 * pin)));
}

pub(crate) const MPC_PWPR: usize = 0xA000_02FF;

pub(crate) const MPC_P00PFS: usize = 0xA000_0200;
pub(crate) const MPC_P01PFS: usize = 0xA000_0201;
pub(crate) const MPC_P02PFS: usize = 0xA000_0202;
pub(crate) const MPC_P03PFS: usize = 0xA000_0203;
pub(crate) const MPC_P04PFS: usize = 0xA000_0204;
pub(crate) const MPC_P05PFS: usize = 0xA000_0205;
pub(crate) const MPC_P06PFS: usize = 0xA000_0206;
pub(crate) const MPC_P07PFS: usize = 0xA000_0207;
pub(crate) const MPC_P10PFS: usize = 0xA000_0208;
pub(crate) const MPC_P15PFS: usize = 0xA000_020D;
pub(crate) const MPC_P24PFS: usize = 0xA000_0214;
pub(crate) const MPC_P36PFS: usize = 0xA000_021E;
pub(crate) const MPC_P37PFS: usize = 0xA000_021F;
pub(crate) const MPC_P46PFS: usize = 0xA000_0226;
pub(crate) const MPC_P90PFS: usize = 0xA000_0248;
pub(crate) const MPC_PE0PFS: usize = 0xA000_0270;
pub(crate) const MPC_PE1PFS: usize = 0xA000_0271;
pub(crate) const MPC_PE2PFS: usize = 0xA000_0272;
pub(crate) const MPC_PE3PFS: usize = 0xA000_0273;
pub(crate) const MPC_PE4PFS: usize = 0xA000_0274;
pub(crate) const MPC_PE5PFS: usize = 0xA000_0275;
pub(crate) const MPC_PE6PFS: usize = 0xA000_0276;
pub(crate) const MPC_PE7PFS: usize = 0xA000_0277;
pub(crate) const MPC_PG0PFS: usize = 0xA000_0280;
pub(crate) const MPC_PG1PFS: usize = 0xA000_0281;
pub(crate) const MPC_PG2PFS: usize = 0xA000_0282;
pub(crate) const MPC_PG3PFS: usize = 0xA000_0283;
pub(crate) const MPC_PG4PFS: usize = 0xA000_0284;
pub(crate) const MPC_PG5PFS: usize = 0xA000_0285;
pub(crate) const MPC_PG6PFS: usize = 0xA000_0286;
pub(crate) const MPC_PG7PFS: usize = 0xA000_0287;
pub(crate) const MPC_PH0PFS: usize = 0xA000_0288;
pub(crate) const MPC_PH1PFS: usize = 0xA000_0289;
pub(crate) const MPC_PH2PFS: usize = 0xA000_028A;
pub(crate) const MPC_PH3PFS: usize = 0xA000_028B;
pub(crate) const MPC_PH4PFS: usize = 0xA000_028C;
pub(crate) const MPC_PH5PFS: usize = 0xA000_028D;
pub(crate) const MPC_PH6PFS: usize = 0xA000_028E;
pub(crate) const MPC_PH7PFS: usize = 0xA000_028F;
pub(crate) const MPC_PK0PFS: usize = 0xA000_0298;

pub(crate) const ECATC_AL_STATUS_CODE: usize = 0xA00D_0134;
pub(crate) const ECATC_DC_SYS_TIME_LO: usize = 0xA00D_0910;
pub(crate) const ECATC_DC_SYS_TIME_HI: usize = 0xA00D_0914;
pub(crate) const ECATC_DC_CYC_START_TIME_LO: usize = 0xA00D_0990;
pub(crate) const ECATC_DC_CYC_START_TIME_HI: usize = 0xA00D_0994;
pub(crate) const ECATC_DC_SYNC0_CYC_TIME: usize = 0xA00D_09A0;
