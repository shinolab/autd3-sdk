use core::arch::asm;

const CPSR_IRQ_MASKED: u32 = 1 << 7;

pub(crate) fn without_irq<R>(f: impl FnOnce() -> R) -> R {
    let cpsr: u32;
    unsafe {
        asm!("mrs {}, cpsr", out(reg) cpsr, options(nomem, nostack, preserves_flags));
        asm!("cpsid i", options(nostack, preserves_flags));
    }
    let result = f();
    if cpsr & CPSR_IRQ_MASKED == 0 {
        unsafe { asm!("cpsie i", options(nostack, preserves_flags)) };
    }
    result
}
