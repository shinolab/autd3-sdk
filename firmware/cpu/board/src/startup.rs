use core::arch::{asm, global_asm};

global_asm!(
    ".arm",
    ".section .fvectors, \"ax\", %progbits",
    ".global _vector_table",
    "_vector_table:",
    "ldr pc, [pc, #24]",
    "ldr pc, [pc, #24]",
    "ldr pc, [pc, #24]",
    "ldr pc, [pc, #24]",
    "ldr pc, [pc, #24]",
    "ldr pc, [pc, #24]",
    "ldr pc, [pc, #24]",
    "ldr pc, [pc, #24]",
    ".word Reset_Handler",
    ".word 3f",
    ".word 3f",
    ".word 3f",
    ".word 3f",
    ".word 3f",
    ".word 3f",
    ".word 3f",
    ".section .text.Reset_Handler, \"ax\", %progbits",
    ".global Reset_Handler",
    ".type Reset_Handler, %function",
    "Reset_Handler:",
    "msr cpsr_c, #{fiq}",
    "ldr sp, =_fiq_stack",
    "msr cpsr_c, #{irq}",
    "ldr sp, =_irq_stack",
    "msr cpsr_c, #{abt}",
    "ldr sp, =_abt_stack",
    "msr cpsr_c, #{und}",
    "ldr sp, =_und_stack",
    "msr cpsr_c, #{svc}",
    "ldr sp, =_svc_stack",
    "msr cpsr_c, #{sys}",
    "ldr sp, =_sys_stack",
    "ldr r0, =_bss",
    "ldr r1, =_ebss",
    "mov r2, #0",
    "2:",
    "cmp r0, r1",
    "strlo r2, [r0], #4",
    "blo 2b",
    "bl {init}",
    "bl main",
    "3:",
    "b 3b",
    ".pool",
    fiq = const MASKED | 0x11,
    irq = const MASKED | 0x12,
    abt = const MASKED | 0x17,
    und = const MASKED | 0x1B,
    svc = const MASKED | 0x13,
    sys = const MASKED | 0x1F,
    init = sym init,
);

const MASKED: u32 = 0xC0;

const SCTLR_M: u32 = 1;
const SCTLR_V: u32 = 1 << 13;
const SCTLR_BR: u32 = 1 << 17;
const SCTLR_VE: u32 = 1 << 24;

const MPU_REGIONS: u32 = 12;

const DRSR_ENABLE: u32 = 1;

const fn drsr(log2_bytes: u32) -> u32 {
    ((log2_bytes - 1) << 1) | DRSR_ENABLE
}

const DRACR_XN: u32 = 1 << 12;
const DRACR_AP_FULL: u32 = 0b011 << 8;
const DRACR_AP_READ_ONLY: u32 = 0b110 << 8;
const DRACR_NORMAL_NON_CACHEABLE: u32 = 0b001 << 3;
const DRACR_DEVICE_NON_SHARED: u32 = 0b010 << 3;

struct Region {
    base: u32,
    size: u32,
    access: u32,
}

const REGIONS: [Region; 5] = [
    Region {
        base: 0x0000_0000,
        size: drsr(19),
        access: DRACR_AP_FULL | DRACR_NORMAL_NON_CACHEABLE,
    },
    Region {
        base: 0x0080_0000,
        size: drsr(15),
        access: DRACR_AP_FULL | DRACR_NORMAL_NON_CACHEABLE,
    },
    Region {
        base: 0x0800_0000,
        size: drsr(27),
        access: DRACR_XN | DRACR_AP_FULL | DRACR_NORMAL_NON_CACHEABLE,
    },
    Region {
        base: 0x3000_0000,
        size: drsr(26),
        access: DRACR_XN | DRACR_AP_READ_ONLY | DRACR_NORMAL_NON_CACHEABLE,
    },
    Region {
        base: 0x4000_0000,
        size: drsr(29),
        access: DRACR_XN | DRACR_AP_FULL | DRACR_DEVICE_NON_SHARED,
    },
];

fn write_sctlr(value: u32) {
    unsafe {
        asm!(
            "dsb",
            "mcr p15, 0, {value}, c1, c0, 0",
            "isb",
            value = in(reg) value,
            options(nostack, preserves_flags),
        );
    }
}

fn write_region(number: u32, base: u32, size: u32, access: u32) {
    unsafe {
        asm!(
            "mcr p15, 0, {number}, c6, c2, 0",
            "mcr p15, 0, {base}, c6, c1, 0",
            "mcr p15, 0, {access}, c6, c1, 4",
            "mcr p15, 0, {size}, c6, c1, 2",
            number = in(reg) number,
            base = in(reg) base,
            access = in(reg) access,
            size = in(reg) size,
            options(nostack, preserves_flags),
        );
    }
}

#[allow(clippy::cast_possible_truncation)]
extern "C" fn init() {
    let sctlr: u32;
    unsafe {
        asm!(
            "mrc p15, 0, {value}, c1, c0, 0",
            value = out(reg) sctlr,
            options(nomem, nostack, preserves_flags),
        );
    }
    let sctlr = (sctlr & !(SCTLR_M | SCTLR_V)) | SCTLR_VE;
    write_sctlr(sctlr);
    for number in REGIONS.len() as u32..MPU_REGIONS {
        write_region(number, 0, 0, 0);
    }
    for (number, region) in REGIONS.iter().enumerate() {
        write_region(number as u32, region.base, region.size, region.access);
    }
    write_sctlr(sctlr | SCTLR_M | SCTLR_BR);
}
