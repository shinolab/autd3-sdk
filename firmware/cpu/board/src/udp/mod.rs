mod ethsw;
mod gmac;
pub(crate) mod irq;
mod pins;
mod pulse;
mod regs;

use core::arch::naked_asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use autd3_cpu_fw::net::{MAX_FRAME, Mac};
use autd3_cpu_fw::nic::{Nic, RxMeta};
use autd3_cpu_fw::node::{CommandLayer, Node, Received};
use autd3_cpu_fw::proto::{Disposition, Reply};
use autd3_cpu_fw::udp::SYNC_CYCLE_NS;
use autd3_cpu_fw::update::{TRANSPORT_MARKER_BYTES, Transport};

use self::regs::{VIC_INTNO_ETHDMAIR, VIC_INTNO_TGIA0};
use crate::bsp::{timer, vic};
use crate::port::HwPort;

#[used]
#[unsafe(link_section = ".image_info")]
static IMAGE_INFO: [u8; TRANSPORT_MARKER_BYTES] = Transport::Udp.marker();

const TX_RING: usize = 8;
const RX_TAG_PORT_BYTE: usize = 4;
const RX_TAG_PORT_BIT: u8 = 7;
const PHY_ADDRESS_BASE: u32 = 1;
const PHY_BMSR: u32 = 1;
const BMSR_LINK_STATUS: u16 = 1 << 2;
const ETHDMAIR_PRIORITY: u32 = 14;
const TGIA0_PRIORITY: u32 = 13;
const SYNC_GUARD_NS: u64 = 250_000;

struct HwNic {
    tx: [u32; TX_RING],
    next_tx: usize,
    frame_id: u32,
}

impl Nic for HwNic {
    fn send(&mut self, frame: &[u8], port: u8) -> bool {
        let Some(buffer) = (0..TX_RING)
            .map(|k| (self.next_tx + k) % TX_RING)
            .find(|&slot| self.tx[slot] != 0)
            .map(|slot| {
                self.next_tx = (slot + 1) % TX_RING;
                self.tx[slot]
            })
        else {
            return false;
        };
        self.frame_id = self.frame_id.wrapping_add(1);
        gmac::send(buffer, frame, self.frame_id, port)
    }

    fn now(&mut self) -> Option<u64> {
        ethsw::capture()
    }

    fn set_time(&mut self, ns: u64) -> bool {
        ethsw::set_time(ns)
    }

    fn set_forwarding(&mut self, open: bool) {
        ethsw::set_forwarding(open);
    }

    fn set_mac(&mut self, mac: Mac) {
        ethsw::set_own_mac(mac);
    }

    fn downstream_link(&mut self, port: u8) -> bool {
        let phy = PHY_ADDRESS_BASE + u32::from(port & 1);
        let _ = gmac::mdio_read(phy, PHY_BMSR);
        gmac::mdio_read(phy, PHY_BMSR).is_some_and(|bmsr| bmsr & BMSR_LINK_STATUS != 0)
    }

    fn arm_pulse(&mut self) {
        pulse::arm();
    }

    fn stop_pulse(&mut self) {
        pulse::stop();
    }

    fn pulse_ready(&mut self) -> bool {
        pulse::is_ready()
    }
}

struct Net {
    node: Node,
    nic: HwNic,
    rx: [u8; MAX_FRAME + 4],
}

struct IsrOnly(UnsafeCell<Net>);

unsafe impl Sync for IsrOnly {}

static NET: IsrOnly = IsrOnly(UnsafeCell::new(Net {
    node: Node::new(),
    nic: HwNic {
        tx: [0; TX_RING],
        next_tx: 0,
        frame_id: 0,
    },
    rx: [0; MAX_FRAME + 4],
}));

static HOST_SEEN: AtomicBool = AtomicBool::new(false);
static LAST_HOST_FRAME_MS: AtomicU32 = AtomicU32::new(0);

fn net() -> &'static mut Net {
    unsafe { &mut *NET.0.get() }
}

pub(crate) fn init() {
    pins::init();
}

pub(crate) fn start() {
    let net = net();
    ethsw::select_switch();
    let _ = ethsw::init_switch();
    ethsw::init_timer();
    gmac::hwf_init();
    let _ = gmac::reset_mac();
    gmac::configure_mac();
    let _ = gmac::rx_enable();
    for buffer in &mut net.nic.tx {
        *buffer = gmac::alloc_tx_buffer().unwrap_or(0);
    }
    net.node.init(&mut net.nic);
    vic::install_with(
        VIC_INTNO_ETHDMAIR,
        ETHDMAIR_PRIORITY,
        ethdmair_entry as *const () as usize,
        false,
    );
    vic::install_with(
        VIC_INTNO_TGIA0,
        TGIA0_PRIORITY,
        tgia0_entry as *const () as usize,
        true,
    );
}

pub(crate) fn tick(now_ms: u32) {
    let net = net();
    net.node.tick(&mut net.nic, now_ms);
}

#[unsafe(naked)]
extern "C" fn ethdmair_entry() {
    naked_asm!(
        ".arm",
        "sub lr, lr, #4",
        "push {{r0-r3, r12, lr}}",
        "bl {isr}",
        "ldm sp!, {{r0-r3, r12, pc}}^",
        isr = sym ethdmair_isr,
    )
}

extern "C" fn ethdmair_isr() {
    let net = net();
    let now_ms = timer::now_ms();
    while let Some(rx) = gmac::poll_rx() {
        if rx.valid {
            let len = gmac::copy_rx(&rx, &mut net.rx);
            if len > RX_TAG_PORT_BYTE {
                let meta = RxMeta {
                    port: (net.rx[RX_TAG_PORT_BYTE] >> RX_TAG_PORT_BIT) & 1,
                };
                let received =
                    net.node
                        .on_frame(&mut net.nic, now_ms, &net.rx[..len], meta, &mut Commands);
                if received == Received::HostMessage {
                    LAST_HOST_FRAME_MS.store(now_ms, Ordering::Relaxed);
                    HOST_SEEN.store(true, Ordering::Release);
                }
            }
        }
        gmac::release(rx.buffer);
    }
    vic::end_of_interrupt();
}

struct Commands;

impl CommandLayer for Commands {
    fn recv_frame(&mut self, frame: &[u8], msg_id: u16) -> Disposition {
        crate::cpu().recv_frame(&mut HwPort, frame, msg_id)
    }

    fn reply(&mut self) -> Reply {
        crate::cpu().reply()
    }
}

pub(crate) fn complete(msg_id: u16) {
    irq::without_irq(|| {
        let net = net();
        let reply = crate::cpu().reply();
        net.node.send_completion(&mut net.nic, msg_id, &reply);
    });
}

#[unsafe(naked)]
extern "C" fn tgia0_entry() {
    naked_asm!(
        ".arm",
        "sub lr, lr, #4",
        "push {{r0-r3, r12, lr}}",
        "bl {isr}",
        "ldm sp!, {{r0-r3, r12, pc}}^",
        isr = sym tgia0_isr,
    )
}

extern "C" fn tgia0_isr() {
    vic::clear_edge(VIC_INTNO_TGIA0);
    pulse::on_capture();
    vic::end_of_interrupt();
}

pub(crate) fn next_sync_edge() -> u64 {
    if !pulse::is_ready() {
        return 0;
    }
    let Some(now) = ethsw::capture() else {
        return 0;
    };
    let cycle = u64::from(SYNC_CYCLE_NS);
    (now + SYNC_GUARD_NS).div_ceil(cycle) * cycle
}

pub(crate) fn sys_time() -> u64 {
    ethsw::capture().unwrap_or(0)
}

pub(crate) fn host_idle_ms() -> Option<u32> {
    if !HOST_SEEN.load(Ordering::Acquire) {
        return None;
    }
    let last = LAST_HOST_FRAME_MS.load(Ordering::Relaxed);
    Some(timer::now_ms().wrapping_sub(last))
}
