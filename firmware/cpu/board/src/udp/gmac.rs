use core::arch::asm;

use super::regs::{
    BUFID, GMAC_ACC, GMAC_MIIM, GMAC_MODE, GMAC_RESET, GMAC_RXMAC_ENA, GMAC_RXMODE, GMAC_TXMODE,
    HWF_C0STAT, HWF_C0TYPE, HWF_CMD, HWF_R0, HWF_R1, HWF_R4, HWF_R5, HWF_R6, HWF_R7, HWF_SYSC,
};
use crate::regs::{read32, write32};

const HWF_TYPE_STAT: u32 = 3;
const HWF_SETUP_CMD: u32 = 0x8004;
const HWF_READY: u32 = 0x8000_0000;
const HWF_COMPLETE: u32 = 1 << 29;
const HWF_MAX_POLLS: u32 = 100_000;
const HWF_SETUP_POLLS: u32 = 1_000;

const HWF_LONG_BUFFER_GET: u32 = 0x5000;
const HWF_BUFFER_RELEASE: u32 = 0x5001;
const HWF_MACDMA_TX_START: u32 = 0x5100;
const HWF_MACDMA_RX_ENABLE: u32 = 0x5101;

const GMAC_RESET_ALL: u32 = 0x8000_0000;
const GMAC_MODE_GIGABIT_FULL_DUPLEX: u32 = 0xC000_0000;
const GMAC_TXMODE_SF_LONG_NO_RESULT: u32 = 0x6000_0080;
const GMAC_RXMODE_ALL_STORE_FORWARD: u32 = 0x2000_0000;

const MIIM_RWDV: u32 = 1 << 26;
const MIIM_MAX_POLLS: u32 = 100_000;

pub(crate) const TX_BUFFER_BYTES: u32 = 2048;
const TX_DESCRIPTOR_OFFSET: u32 = 0;
const TX_DATA_OFFSET: u32 = 64;
const TX_CONTROL_BYTES: u32 = 8;
const TX_DESCRIPTOR_END: u32 = 0xFFFF_FFFF;

const TXCTL_PORT_SHIFT: u32 = 9;
const TXCTL_FORCED_FORWARDING: u32 = 1 << 8;
const TXCTL_TIMESTAMP: u32 = 1 << 7;
const TXCTL_TCPIP_ACC_OFF: u32 = 1 << 5;
const TXCTL_APAD: u32 = 1 << 2;

const BUFID_NOT_EMPTY: u32 = 1 << 31;
const BUFID_VALID: u32 = 1 << 28;
const BUFFER_RAM_BASE: u32 = 0x0800_0000;
const RX_INFO_WORDS: u32 = 2;

fn dmb() {
    unsafe { asm!("dmb", options(nostack, preserves_flags)) };
}

pub(crate) fn hwf_init() {
    write32(HWF_C0TYPE, HWF_TYPE_STAT);
    write32(HWF_C0STAT, HWF_TYPE_STAT);
    write32(HWF_CMD, HWF_SETUP_CMD);
    for _ in 0..HWF_SETUP_POLLS {
        let r0 = read32(HWF_R0);
        let _ = read32(HWF_R1);
        if r0 == HWF_READY {
            break;
        }
    }
}

fn hwf_call(sysc: u32, args: [u32; 4]) -> Option<(u32, u32)> {
    write32(HWF_R4, args[0]);
    write32(HWF_R5, args[1]);
    write32(HWF_R6, args[2]);
    write32(HWF_R7, args[3]);
    dmb();
    write32(HWF_SYSC, sysc);
    for _ in 0..HWF_MAX_POLLS {
        let r0 = read32(HWF_R0);
        if r0 & HWF_COMPLETE != 0 {
            return Some((r0, read32(HWF_R1)));
        }
    }
    None
}

pub(crate) fn reset_mac() -> bool {
    write32(GMAC_RESET, GMAC_RESET_ALL);
    (0..HWF_MAX_POLLS).any(|_| read32(GMAC_RESET) == 0)
}

pub(crate) fn configure_mac() {
    write32(GMAC_MODE, GMAC_MODE_GIGABIT_FULL_DUPLEX);
    write32(GMAC_TXMODE, GMAC_TXMODE_SF_LONG_NO_RESULT);
    write32(GMAC_RXMODE, GMAC_RXMODE_ALL_STORE_FORWARD);
    write32(GMAC_ACC, 0);
    write32(GMAC_RXMAC_ENA, 1);
}

pub(crate) fn rx_enable() -> bool {
    hwf_call(HWF_MACDMA_RX_ENABLE, [0; 4]).is_some_and(|(r0, _)| r0 & 1 == 0)
}

pub(crate) fn alloc_tx_buffer() -> Option<u32> {
    let (r0, r1) = hwf_call(HWF_LONG_BUFFER_GET, [TX_BUFFER_BYTES, 0, 0, 0])?;
    (r0 & 0b10 == 0).then_some(r1)
}

pub(crate) fn release(buffer: u32) {
    let _ = hwf_call(HWF_BUFFER_RELEASE, [buffer, 0, 0, 0]);
}

fn write_word(addr: u32, value: u32) {
    unsafe { (addr as usize as *mut u32).write_volatile(value) }
}

fn read_word(addr: u32) -> u32 {
    unsafe { (addr as usize as *const u32).read_volatile() }
}

pub(crate) fn send(buffer: u32, frame: &[u8], frame_id: u32, port: u8, timestamp: bool) -> bool {
    let Ok(len) = u32::try_from(frame.len()) else {
        return false;
    };
    if TX_DATA_OFFSET + TX_CONTROL_BYTES + len > TX_BUFFER_BYTES {
        return false;
    }
    let mut flags = ((1 << (port & 1)) << TXCTL_PORT_SHIFT)
        | TXCTL_FORCED_FORWARDING
        | TXCTL_TCPIP_ACC_OFF
        | TXCTL_APAD;
    if timestamp {
        flags |= TXCTL_TIMESTAMP;
    }
    let data = buffer + TX_DATA_OFFSET;
    write_word(data, ((len + 3) << 16) | flags);
    write_word(data + 4, frame_id);
    for (offset, chunk) in (0u32..).step_by(4).zip(frame.chunks(4)) {
        let mut bytes = [0u8; 4];
        bytes[..chunk.len()].copy_from_slice(chunk);
        write_word(data + TX_CONTROL_BYTES + offset, u32::from_le_bytes(bytes));
    }
    let descriptor = buffer + TX_DESCRIPTOR_OFFSET;
    write_word(descriptor, data);
    write_word(descriptor + 4, TX_CONTROL_BYTES + len);
    write_word(descriptor + 8, TX_DESCRIPTOR_END);
    write_word(descriptor + 12, 0);
    hwf_call(HWF_MACDMA_TX_START, [descriptor, 0, 0, 0]).is_some_and(|(r0, _)| r0 & 1 == 0)
}

pub(crate) struct Received {
    pub(crate) buffer: u32,
    pub(crate) len: usize,
    pub(crate) valid: bool,
}

pub(crate) fn poll_rx() -> Option<Received> {
    let bufid = read32(BUFID);
    if bufid & BUFID_NOT_EMPTY == 0 {
        return None;
    }
    let words = (bufid >> 16) & 0xFFF;
    Some(Received {
        buffer: BUFFER_RAM_BASE + ((bufid & 0xFFFF) << 11),
        len: (words.saturating_sub(RX_INFO_WORDS) * 4) as usize,
        valid: bufid & BUFID_VALID != 0,
    })
}

pub(crate) fn copy_rx(rx: &Received, dst: &mut [u8]) -> usize {
    let len = rx.len.min(dst.len());
    for (offset, chunk) in (0u32..).step_by(4).zip(dst[..len].chunks_mut(4)) {
        let word = read_word(rx.buffer + offset).to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
    len
}

pub(crate) fn mdio_read(phy: u32, reg: u32) -> Option<u16> {
    write32(GMAC_MIIM, ((phy & 0x1F) << 21) | ((reg & 0x1F) << 16));
    for _ in 0..MIIM_MAX_POLLS {
        let v = read32(GMAC_MIIM);
        if v & MIIM_RWDV != 0 {
            return u16::try_from(v & 0xFFFF).ok();
        }
    }
    None
}
