mod ethsw;
mod gmac;
pub(crate) mod irq;
mod pins;
mod pulse;
mod regs;

use core::cell::UnsafeCell;
use core::num::{NonZeroU32, NonZeroU64};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use autd3_cpu_fw::net::Mac;
use autd3_cpu_fw::nic::{Nic, RxMeta, TxStamp};
use autd3_cpu_fw::node::{Node, Received};
use autd3_cpu_fw::udp::SYNC_CYCLE_NS;

use self::regs::{VIC_INTNO_ETHDMAIR, VIC_INTNO_TGIA0};
use crate::bsp::{io, timer, vic};

const TX_RING: usize = 32;
const RX_TAG_TIMESTAMP_BYTES: usize = 4;
const RX_TAG_PORT_BYTE: usize = 4;
const RX_TAG_PORT_BIT: u8 = 7;
const PHY_ADDRESS_BASE: u32 = 1;
const PHY_BMCR: u32 = 0;
const PHY_BMSR: u32 = 1;
const PHY_ANAR: u32 = 4;
const BMCR_RESTART_AUTONEG: u16 = 1 << 9;
const BMCR_AUTONEG_ENABLE: u16 = 1 << 12;
const BMSR_LINK_STATUS: u16 = 1 << 2;
const ANAR_PAUSE: u16 = 1 << 10;
const ETHDMAIR_PRIORITY: u32 = 14;
const TGIA0_PRIORITY: u32 = 13;

struct HwNic {
    tx: [Option<NonZeroU32>; TX_RING],
    next_tx: usize,
    frame_id: u32,
}

impl Nic for HwNic {
    fn send(&mut self, frame: &[u8], port: u8, timestamp: bool) -> bool {
        let Some(buffer) = (0..TX_RING)
            .map(|k| (self.next_tx + k) % TX_RING)
            .find_map(|slot| self.tx[slot].map(|buffer| (slot, buffer)))
            .map(|(slot, buffer)| {
                self.next_tx = (slot + 1) % TX_RING;
                buffer
            })
        else {
            return false;
        };
        self.frame_id = self.frame_id.wrapping_add(1);
        gmac::send(buffer.get(), frame, self.frame_id, port, timestamp)
    }

    fn now(&mut self) -> Option<u64> {
        ethsw::capture()
    }

    fn step(&mut self, offset_ns: i64) -> bool {
        ethsw::step_time(offset_ns)
    }

    fn set_time(&mut self, ns: u64) -> bool {
        ethsw::set_time(ns)
    }

    fn set_drift(&mut self, ppb: i32) {
        ethsw::set_drift(ppb);
    }

    fn clear_tx_timestamps(&mut self) {
        ethsw::clear_tx_timestamps();
    }

    fn take_tx_timestamp(&mut self, port: u8) -> Option<TxStamp> {
        ethsw::take_tx_timestamp(port)
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

    fn arm_pulse(&mut self) -> bool {
        pulse::arm()
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
    rx: gmac::RxBuffer,
}

struct IsrOnly(UnsafeCell<Net>);

unsafe impl Sync for IsrOnly {}

static NET: IsrOnly = IsrOnly(UnsafeCell::new(Net {
    node: Node::new(),
    nic: HwNic {
        tx: [None; TX_RING],
        next_tx: 0,
        frame_id: 0,
    },
    rx: gmac::RxBuffer([0; gmac::RX_BUFFER_BYTES]),
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
    let switch_ready = ethsw::init_switch();
    ethsw::init_timer();
    gmac::hwf_init();
    let mac_reset = gmac::reset_mac();
    gmac::configure_mac();
    let rx_enabled = gmac::rx_enable();
    for ok in [switch_ready, mac_reset, rx_enabled] {
        if !ok {
            crate::cpu().count_boot_failure();
            io::show_network_fault();
        }
    }
    advertise_pause();
    for buffer in &mut net.nic.tx {
        *buffer = gmac::alloc_tx_buffer().and_then(NonZeroU32::new);
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

fn advertise_pause() {
    for port in 0..2 {
        let phy = PHY_ADDRESS_BASE + port;
        let Some(anar) = gmac::mdio_read(phy, PHY_ANAR) else {
            continue;
        };
        if anar & ANAR_PAUSE != 0 {
            continue;
        }
        let Some(bmcr) = gmac::mdio_read(phy, PHY_BMCR) else {
            continue;
        };
        if gmac::mdio_write(phy, PHY_ANAR, anar | ANAR_PAUSE) {
            let _ = gmac::mdio_write(
                phy,
                PHY_BMCR,
                bmcr | BMCR_AUTONEG_ENABLE | BMCR_RESTART_AUTONEG,
            );
        }
    }
}

pub(crate) fn tick(now_ms: u32) {
    let net = net();
    net.node.tick(&mut net.nic, now_ms, &mut crate::cpu());
}

vic::irq_entry!(ethdmair_entry, ethdmair_isr);

extern "C" fn ethdmair_isr() {
    let net = net();
    let now_ms = timer::now_ms();
    while let Some(rx) = gmac::poll_rx() {
        if rx.valid {
            let len = gmac::copy_rx(&rx, &mut net.rx);
            if len > RX_TAG_PORT_BYTE {
                let mut timestamp = [0u8; RX_TAG_TIMESTAMP_BYTES];
                timestamp.copy_from_slice(&net.rx.0[..RX_TAG_TIMESTAMP_BYTES]);
                let meta = RxMeta {
                    port: (net.rx.0[RX_TAG_PORT_BYTE] >> RX_TAG_PORT_BIT) & 1,
                    timestamp_ns: u32::from_le_bytes(timestamp),
                };
                let received = net.node.on_frame(
                    &mut net.nic,
                    now_ms,
                    &net.rx.0[..len],
                    meta,
                    &mut crate::cpu(),
                );
                if received == Received::HostMessage {
                    LAST_HOST_FRAME_MS.store(now_ms, Ordering::Relaxed);
                    HOST_SEEN.store(true, Ordering::Release);
                }
            }
        }
        gmac::release(rx.buffer);
        pulse::service();
    }
    vic::end_of_interrupt();
}

pub(crate) fn complete(msg_id: u16) {
    irq::without_irq(|| {
        let net = net();
        net.node
            .send_completion(&mut net.nic, msg_id, &mut crate::cpu());
    });
}

vic::irq_entry!(tgia0_entry, tgia0_isr);

extern "C" fn tgia0_isr() {
    vic::clear_edge(VIC_INTNO_TGIA0);
    pulse::on_capture();
    vic::end_of_interrupt();
}

pub(crate) fn set_fpga_bus_wait(cycles: u32) {
    pins::set_cs1_wait(cycles);
}

pub(crate) fn configure_ptp(config: autd3_cpu_fw::ptp::Config) {
    irq::without_irq(|| net().node.configure_ptp(config));
}

pub(crate) fn next_sync_edge(guard_ns: u32) -> Option<NonZeroU64> {
    if !pulse::is_ready() {
        return None;
    }
    let now = ethsw::capture()?;
    let cycle = u64::from(SYNC_CYCLE_NS);
    NonZeroU64::new((now + u64::from(guard_ns)).div_ceil(cycle) * cycle)
}

pub(crate) fn sys_time() -> Option<u64> {
    ethsw::capture()
}

pub(crate) fn host_idle_ms() -> Option<u32> {
    if !HOST_SEEN.load(Ordering::Acquire) {
        return None;
    }
    let last = LAST_HOST_FRAME_MS.load(Ordering::Relaxed);
    Some(timer::now_ms().wrapping_sub(last))
}

pub(crate) fn ptp_unlocked_ms() -> Option<u32> {
    irq::without_irq(|| net().node.ptp_unlocked_ms(timer::now_ms()))
}
