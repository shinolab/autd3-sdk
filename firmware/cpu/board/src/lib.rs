#![no_std]

mod bsp;
mod port;
mod regs;
mod udp;

use core::arch::asm;
use core::panic::PanicInfo;

use autd3_cpu_fw::Cpu;
use autd3_cpu_fw::proto::Drained;

use crate::port::HwPort;

struct StaticCpu(Cpu);

unsafe impl Sync for StaticCpu {}

static CPU: StaticCpu = StaticCpu(Cpu::new());

fn cpu() -> &'static Cpu {
    &CPU.0
}

unsafe extern "C" {
    fn sflash_init();
}

const PRCR_UNLOCK_ALL: u32 = 0x0000_A503;
const FPGA_STARTUP_MS: u16 = 200;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    bsp::io::init();
    bsp::bus::init();
    bsp::clock::init();
    bsp::vic::init();
    bsp::timer::init();
    regs::write32(regs::SYSTEM_PRCR, PRCR_UNLOCK_ALL);
    let _ = regs::read32(regs::SYSTEM_PRCR);
    bsp::io::leds_on();
    bsp::timer::delay_ms(FPGA_STARTUP_MS);
    udp::init();
    unsafe { sflash_init() };
    cpu().mark_boot_attempt(&mut HwPort);
    cpu().init(&mut HwPort);
    udp::start();
    bsp::vic::irq_enable();
    loop {
        loop {
            match cpu().process_one(&mut HwPort) {
                Drained::Empty => break,
                Drained::Completed { msg_id } => udp::complete(msg_id),
                Drained::Flushed => {}
            }
        }
        for _ in 0..bsp::timer::elapsed_ms() {
            cpu().tick_1ms(&mut HwPort);
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { asm!("cpsid i", options(nomem, nostack, preserves_flags)) };
    loop {
        core::hint::spin_loop();
    }
}
