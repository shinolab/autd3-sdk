#![no_std]

mod bsp;
mod port;
mod regs;

use core::arch::asm;
use core::panic::PanicInfo;

use autd3_cpu_fw::Cpu;
use autd3_cpu_fw::proto::{TxFrame, WIRE_RX_FRAME_BYTES};

use crate::port::HwPort;

#[repr(C)]
struct TxWire {
    _reserved: u16,
    ack_data: u16,
}

unsafe extern "C" {
    static mut _sTx: TxWire;
}

struct StaticCpu(Cpu);

// SAFETY: this single-core firmware has exactly two execution contexts: the EtherCAT
// host-interface ISR (`recv_ethercat`) and the main loop (`init_app` / `app_process_pending`),
// and an ISR is sequenced with the code it preempts. All `Cpu` state that both contexts
// touch is atomic (`tx`, `expected_seq`, mode/dedup flags, and the FIFO indices, whose
// stores publish the slot contents with Release/Acquire). The `Cell` state (FIFO slots,
// dispatch bookkeeping) is only accessed by the SPSC owner of the slot or by the single
// context that may dispatch at a time (inline dispatch requires an empty ring; the frame
// consumed by the main loop stays unconsumed until its dispatch returns). `Cpu` exposes
// no `&mut self` API, so no exclusive reference is ever formed.
unsafe impl Sync for StaticCpu {}

static CPU: StaticCpu = StaticCpu(Cpu::new());

fn publish_tx(tx: TxFrame) {
    let packed = u16::from(tx.ack) | (u16::from(tx.data) << 8);
    // SAFETY: `_sTx` is defined by platform.o, 2-byte aligned by its layout, and copied to
    // the TxPDO by the host-interface ISR. A single volatile 16-bit store keeps ack and
    // data consistent in every TxPDO snapshot.
    unsafe { (&raw mut _sTx.ack_data).write_volatile(packed) };
}

#[unsafe(no_mangle)]
pub extern "C" fn init_app() {
    CPU.0.init(&mut HwPort);
    publish_tx(CPU.0.tx());
}

#[unsafe(no_mangle)]
pub extern "C" fn recv_ethercat(frame: *const u8) {
    // SAFETY: platform.o hands us the RxPDO image, which is `WIRE_RX_FRAME_BYTES` long and
    // stays valid for the duration of this call.
    let frame = unsafe { &*(frame.cast::<[u8; WIRE_RX_FRAME_BYTES]>()) };
    CPU.0.recv_ethercat(&mut HwPort, frame);
    publish_tx(CPU.0.tx());
}

#[unsafe(no_mangle)]
pub extern "C" fn app_process_pending() {
    while CPU.0.process_one(&mut HwPort) {
        publish_tx(CPU.0.tx());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_clock_init() {
    bsp::clock::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_bus_init() {
    bsp::bus::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_io_init() {
    bsp::io::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_vic_init() {
    bsp::vic::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_timer_init() {
    bsp::timer::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_irq_enable() {
    bsp::vic::irq_enable();
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_delay_ms(ms: u16) {
    bsp::timer::delay_ms(ms);
}

#[unsafe(no_mangle)]
pub extern "C" fn bsp_vic_install(intno: u32, priority: u32, handler: Option<extern "C" fn()>) {
    bsp::vic::install(intno, priority, handler.map_or(0, |f| f as usize));
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // SAFETY: `cpsid i` masks IRQs on the current core so the EtherCAT ISR stops running
    // against the panicked state and stops acking the master; the device then visibly
    // drops off the bus instead of limping on.
    unsafe { asm!("cpsid i", options(nomem, nostack, preserves_flags)) };
    loop {
        core::hint::spin_loop();
    }
}
