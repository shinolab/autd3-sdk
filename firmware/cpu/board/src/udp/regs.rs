pub(crate) const MSTPCRA: usize = 0xA00B_0300;
pub(crate) const MSTPCRB: usize = 0xA00B_0304;
pub(crate) const MSTPCRC: usize = 0xA00B_0308;
pub(crate) const SCKCR: usize = 0xA00B_0020;

pub(crate) const CS1BCR: usize = 0xA000_2008;
pub(crate) const CS1WCR: usize = 0xA000_202C;

pub(crate) const fn pfs(port: usize, pin: usize) -> usize {
    0xA000_0200 + port * 8 + pin
}

pub(crate) const fn pmr(port: usize) -> usize {
    0xA000_0080 + port
}

pub(crate) const PORT_2: usize = 2;
pub(crate) const PORT_4: usize = 4;
pub(crate) const PORT_5: usize = 5;
pub(crate) const PORT_8: usize = 8;
pub(crate) const PORT_B: usize = 11;
pub(crate) const PORT_C: usize = 12;
pub(crate) const PORT_D: usize = 13;
pub(crate) const PORT_F: usize = 15;
pub(crate) const PORT_J: usize = 18;
pub(crate) const PORT_M: usize = 21;
pub(crate) const PORT_U: usize = 27;

pub(crate) const ETSPCMD: usize = 0xA00B_F000;
pub(crate) const MACSEL: usize = 0xA00B_F004;
pub(crate) const MII_CTRL0: usize = 0xA00B_F008;
pub(crate) const MII_CTRL1: usize = 0xA00B_F00C;
pub(crate) const ETHPHYLNK: usize = 0xA00B_F014;
pub(crate) const ETHSWMTC: usize = 0xA00B_F110;
pub(crate) const ETHSWMD: usize = 0xA00B_F114;
pub(crate) const ETHSFTRST: usize = 0xA00B_F118;
pub(crate) const SWTMEN: usize = 0xA00B_F200;
pub(crate) const SWTMSTSEC: usize = 0xA00B_F204;
pub(crate) const SWTMSTNS: usize = 0xA00B_F208;
pub(crate) const SWTMPSEC: usize = 0xA00B_F20C;
pub(crate) const SWTMPNS: usize = 0xA00B_F210;
pub(crate) const SWTMLATSEC: usize = 0xA00B_F22C;
pub(crate) const SWTMLATNS: usize = 0xA00B_F230;

pub(crate) const SPCMD: usize = 0xA00F_2100;
pub(crate) const EMACRST: usize = 0xA00F_2110;

pub(crate) const GMAC_MODE: usize = 0xA00F_0020;
pub(crate) const GMAC_RXMODE: usize = 0xA00F_0024;
pub(crate) const GMAC_TXMODE: usize = 0xA00F_0028;
pub(crate) const GMAC_RESET: usize = 0xA00F_0030;
pub(crate) const GMAC_MIIM: usize = 0xA00F_00A0;
pub(crate) const GMAC_ACC: usize = 0xA00F_0208;
pub(crate) const GMAC_RXMAC_ENA: usize = 0xA00F_0220;
pub(crate) const BUFID: usize = 0xA00F_1100;

pub(crate) const HWF_C0TYPE: usize = 0xA00E_0000;
pub(crate) const HWF_C0STAT: usize = 0xA00E_0008;
pub(crate) const HWF_SYSC: usize = 0xA00E_F000;
pub(crate) const HWF_R4: usize = 0xA00E_F004;
pub(crate) const HWF_R5: usize = 0xA00E_F008;
pub(crate) const HWF_R6: usize = 0xA00E_F00C;
pub(crate) const HWF_R7: usize = 0xA00E_F010;
pub(crate) const HWF_CMD: usize = 0xA00E_F014;
pub(crate) const HWF_R0: usize = 0xA00E_F020;
pub(crate) const HWF_R1: usize = 0xA00E_F024;

pub(crate) const SW_PORT_ENA: usize = 0xA00C_0008;
pub(crate) const SW_UCAST_DEFAULT_MASK: usize = 0xA00C_000C;
pub(crate) const SW_BCAST_DEFAULT_MASK: usize = 0xA00C_0014;
pub(crate) const SW_MCAST_DEFAULT_MASK: usize = 0xA00C_0018;
pub(crate) const SW_INPUT_LEARN_BLOCK: usize = 0xA00C_001C;
pub(crate) const SW_MGMT_CONFIG: usize = 0xA00C_0020;
pub(crate) const SW_OQMGR_STATUS: usize = 0xA00C_0080;
pub(crate) const SW_QMGR_ST_MINCELLS: usize = 0xA00C_0088;
pub(crate) const SW_QMGR_WEIGHTS: usize = 0xA00C_0094;
pub(crate) const SW_HUB_CONTROL: usize = 0xA00C_01C0;
pub(crate) const SW_ADR_TABLE: usize = 0xA00C_4000;
pub(crate) const SW_ADR_TABLE_ENTRIES: usize = 256;
pub(crate) const SW_MAC_BASE: [usize; 2] = [0xA00C_8000, 0xA00C_A000];
pub(crate) const SW_MAC_COMMAND_CONFIG: usize = 0x08;
pub(crate) const SW_MAC_FRM_LENGTH: usize = 0x14;
pub(crate) const SW_MAC_TX_SECTION_EMPTY: usize = 0x24;
pub(crate) const SW_MAC_TX_SECTION_FULL: usize = 0x28;

pub(crate) const TSM_CONFIG: usize = 0xA00C_C004;
pub(crate) const TSM_IRQ_STAT_ACK: usize = 0xA00C_C008;
pub(crate) const PORT0_CTRL: usize = 0xA00C_C020;
pub(crate) const PORT1_CTRL: usize = 0xA00C_C028;
pub(crate) const ATIME_CTRL: usize = 0xA00C_C120;
pub(crate) const ATIME: usize = 0xA00C_C124;
pub(crate) const ATIME_EVT_PERIOD: usize = 0xA00C_C12C;
pub(crate) const ATIME_CORR: usize = 0xA00C_C130;
pub(crate) const ATIME_INC: usize = 0xA00C_C134;
pub(crate) const ATIME_SEC: usize = 0xA00C_C138;

pub(crate) const MTU_TSTRA: usize = 0xA006_A080;
pub(crate) const MTU0_TCR: usize = 0xA006_A100;
pub(crate) const MTU0_TMDR1: usize = 0xA006_A101;
pub(crate) const MTU0_TIORH: usize = 0xA006_A102;
pub(crate) const MTU0_TIER: usize = 0xA006_A104;
pub(crate) const MTU0_TCNT: usize = 0xA006_A106;
pub(crate) const MTU0_TGRA: usize = 0xA006_A108;
pub(crate) const MTU0_TCR2: usize = 0xA006_A128;

pub(crate) const PPG1_PCR: usize = 0xA008_0516;
pub(crate) const PPG1_PMR: usize = 0xA008_0517;
pub(crate) const PPG1_NDERL: usize = 0xA008_0519;
pub(crate) const PPG1_PODRL: usize = 0xA008_051B;
pub(crate) const PPG1_NDRL: usize = 0xA008_051D;
pub(crate) const PPG1_PTRSLR: usize = 0xA008_0520;

pub(crate) const ELC_ELCR: usize = 0xA008_0B00;
pub(crate) const ELC_ELSR0: usize = 0xA008_0B01;
pub(crate) const ELC_ELOPA: usize = 0xA008_0B1F;

pub(crate) const VIC_INTNO_ETHDMAIR: u32 = 51;
pub(crate) const VIC_INTNO_TGIA0: u32 = 145;
