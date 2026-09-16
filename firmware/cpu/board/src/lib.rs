#![no_std]

mod bsp;
mod port;
mod regs;

use core::arch::asm;
use core::panic::PanicInfo;

use autd3_cpu_fw::Cpu;
use autd3_cpu_fw::proto::WIRE_RX_FRAME_BYTES;

use crate::port::HwPort;

struct StaticCpu(Cpu);

unsafe impl Sync for StaticCpu {}

static CPU: StaticCpu = StaticCpu(Cpu::new());

#[unsafe(no_mangle)]
pub extern "C" fn init_app() {
    CPU.0.init(&mut HwPort);
}

#[unsafe(no_mangle)]
pub extern "C" fn recv_ethercat(frame: *const u8) {
    #[cfg(feature = "isr-probe")]
    bsp::io::isr_probe_high();
    let frame = unsafe { &*(frame.cast::<[u8; WIRE_RX_FRAME_BYTES]>()) };
    CPU.0.recv_ethercat(&mut HwPort, frame);
    #[cfg(feature = "isr-probe")]
    bsp::io::isr_probe_low();
}

#[unsafe(no_mangle)]
pub extern "C" fn app_process_pending() {
    CPU.0.process_pending(&mut HwPort);
}

#[unsafe(no_mangle)]
pub extern "C" fn app_tick() {
    for _ in 0..bsp::timer::elapsed_ms() {
        CPU.0.tick_1ms(&mut HwPort);
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
    unsafe { asm!("cpsid i", options(nomem, nostack, preserves_flags)) };
    loop {
        core::hint::spin_loop();
    }
}
