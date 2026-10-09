use super::regs::{
    BUFID, GMAC_ACC, GMAC_MIIM, GMAC_MODE, GMAC_RESET, GMAC_RXMAC_ENA, GMAC_RXMODE, GMAC_TXMODE,
    HWF_C0STAT, HWF_C0TYPE, HWF_CMD, HWF_R0, HWF_R1, HWF_R4, HWF_R5, HWF_R6, HWF_R7, HWF_SYSC,
};
use crate::regs::{dmb, read32, write32};
use autd3_cpu_fw::net::MAX_FRAME;

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
    write32(data as usize, ((len + 3) << 16) | flags);
    write32((data + 4) as usize, frame_id);
    let payload = (data + TX_CONTROL_BYTES) as usize;
    let (words, tail) = frame.as_chunks::<4>();
    for (i, word) in words.iter().enumerate() {
        write32(payload + 4 * i, u32::from_le_bytes(*word));
    }
    if !tail.is_empty() {
        let mut bytes = [0u8; 4];
        for (dst, src) in bytes.iter_mut().zip(tail) {
            *dst = *src;
        }
        write32(payload + 4 * words.len(), u32::from_le_bytes(bytes));
    }
    let descriptor = buffer + TX_DESCRIPTOR_OFFSET;
    write32(descriptor as usize, data);
    write32((descriptor + 4) as usize, TX_CONTROL_BYTES + len);
    write32((descriptor + 8) as usize, TX_DESCRIPTOR_END);
    write32((descriptor + 12) as usize, 0);
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

pub(crate) const RX_BUFFER_BYTES: usize = (MAX_FRAME + 4).next_multiple_of(4);

#[repr(C, align(4))]
pub(crate) struct RxBuffer(pub(crate) [u8; RX_BUFFER_BYTES]);

pub(crate) fn copy_rx(rx: &Received, dst: &mut RxBuffer) -> usize {
    let len = rx.len.min(RX_BUFFER_BYTES);
    let (words, _) = dst.0[..len].as_chunks_mut::<4>();
    for (i, word) in words.iter_mut().enumerate() {
        *word = read32(rx.buffer as usize + 4 * i).to_le_bytes();
    }
    4 * words.len()
}

pub(crate) fn mdio_write(phy: u32, reg: u32, value: u16) -> bool {
    write32(
        GMAC_MIIM,
        MIIM_RWDV | ((phy & 0x1F) << 21) | ((reg & 0x1F) << 16) | u32::from(value),
    );
    (0..MIIM_MAX_POLLS).any(|_| read32(GMAC_MIIM) & MIIM_RWDV != 0)
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
