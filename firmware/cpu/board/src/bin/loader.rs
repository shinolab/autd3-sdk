#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

use autd3_cpu_fw::update::{
    CRC32, IMAGE_APP_LOAD_ADDR, IMAGE_RESET_VECTOR_OFFSET, IMAGE_VECTOR_BYTES, ImageHeader, Slot,
    select_boot_slot,
};

const BTCM_LOADER_BASE: u32 = 0x0080_2000;
const LOADER_STACK_TOP: u32 = BTCM_LOADER_BASE;
const SERIAL_FLASH_BASE: u32 = 0x3000_0000;

const PARAM_WORDS: usize = 19;
const PARAM_CACHE_DISABLED: u32 = 0;
const PARAM_SSLDR: u32 = 0x0007_0707;
const PARAM_SPBCR: u32 = 0x0000_0003;
const PARAM_DRCR: u32 = 0;
const PARAM_SPIBSC_KEEP: u32 = 0;
const PARAM_LOADER_FLASH_ADDR: u32 = SERIAL_FLASH_BASE + 0x4C;
const _: () = assert!(size_of::<[u32; PARAM_WORDS]>() == 0x4C);
const PARAM_LOADER_BYTES: u32 = 0x6000;

const fn loader_param() -> [u32; PARAM_WORDS] {
    let mut param = [0; PARAM_WORDS];
    param[0] = PARAM_CACHE_DISABLED;
    param[1] = PARAM_SSLDR;
    param[2] = PARAM_SPBCR;
    param[3] = PARAM_DRCR;
    param[4] = PARAM_SPIBSC_KEEP;
    param[5] = PARAM_LOADER_FLASH_ADDR;
    param[6] = PARAM_LOADER_BYTES;
    param[7] = BTCM_LOADER_BASE;
    let mut sum = 0;
    let mut i = 0;
    while i < PARAM_WORDS - 1 {
        sum += (param[i] >> 16) + (param[i] & 0xFFFF);
        i += 1;
    }
    param[PARAM_WORDS - 1] = sum;
    param
}

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".loader_param")]
static LOADER_PARAM: [u32; PARAM_WORDS] = loader_param();

const _: () = assert!(LOADER_PARAM[PARAM_WORDS - 1] == 0x0000_B7DD);

global_asm!(
    ".arm",
    ".section .loader_entry, \"ax\", %progbits",
    ".global _PowerON_Reset",
    ".type _PowerON_Reset, %function",
    "_PowerON_Reset:",
    "ldr sp, ={stack}",
    "bl {main}",
    "2:",
    "b 2b",
    ".pool",
    stack = const LOADER_STACK_TOP,
    main = sym loader_main,
);

const SYSTEM_PRCR: usize = 0xA00B_0B00;
const SYSTEM_SYTATCMWAIT: usize = 0xA00B_0800;
const PRCR_ATCM_WAIT_UNLOCK: u32 = 0x0000_A508;
const PRCR_LOCK: u32 = 0x0000_A500;
const ATCMWAIT_ONE_WAIT_OPTIMIZED: u32 = 0;

const VECTOR_WORDS: usize = IMAGE_VECTOR_BYTES as usize / 4;

fn read32(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

fn write32(addr: usize, value: u32) {
    unsafe { (addr as *mut u32).write_volatile(value) }
}

fn read_header(slot: Slot) -> Option<ImageHeader> {
    let addr = (SERIAL_FLASH_BASE + slot.base()) as usize;
    let header = unsafe { (addr as *const ImageHeader).read_volatile() };
    header.is_plausible().then_some(header)
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

extern "C" fn loader_main() -> ! {
    write32(SYSTEM_PRCR, PRCR_ATCM_WAIT_UNLOCK);
    let _ = read32(SYSTEM_PRCR);
    write32(SYSTEM_SYTATCMWAIT, ATCMWAIT_ONE_WAIT_OPTIMIZED);
    let _ = read32(SYSTEM_SYTATCMWAIT);
    write32(SYSTEM_PRCR, PRCR_LOCK);
    let _ = read32(SYSTEM_PRCR);
    let mut a = read_header(Slot::A);
    let mut b = read_header(Slot::B);
    loop {
        let (slot, header) = match select_boot_slot(a.as_ref(), b.as_ref()) {
            Some(Slot::A) => (Slot::A, a.take()),
            Some(Slot::B) => (Slot::B, b.take()),
            None => halt(),
        };
        let Some(header) = header else {
            continue;
        };
        let app_bytes = (header.length.get() - IMAGE_VECTOR_BYTES) as usize;
        let base = (SERIAL_FLASH_BASE + slot.image_base()) as usize;
        let mut vector = [0; VECTOR_WORDS];
        for (i, word) in vector.iter_mut().enumerate() {
            *word = read32(base + 4 * i);
        }
        let src = (SERIAL_FLASH_BASE + slot.image_base() + IMAGE_VECTOR_BYTES) as usize;
        let dst = IMAGE_APP_LOAD_ADDR as usize;
        for offset in (0..app_bytes).step_by(4) {
            write32(dst + offset, read32(src + offset));
        }
        let app =
            unsafe { core::slice::from_raw_parts(IMAGE_APP_LOAD_ADDR as *const u8, app_bytes) };
        let mut digest = CRC32.digest();
        for word in vector {
            digest.update(&word.to_le_bytes());
        }
        digest.update(app);
        if digest.finalize() != header.crc32.get() {
            continue;
        }
        unsafe {
            asm!(
                "2:",
                "ldr r3, [r0], #4",
                "str r3, [r1], #4",
                "subs r2, r2, #1",
                "bne 2b",
                "mrc p15, 0, r3, c1, c0, 0",
                "bic r3, r3, #0x2000",
                "mcr p15, 0, r3, c1, c0, 0",
                "isb",
                "mov r3, #{reset_vector}",
                "ldr r3, [r3]",
                "bx r3",
                in("r0") vector.as_ptr(),
                in("r1") 0u32,
                in("r2") VECTOR_WORDS,
                reset_vector = const IMAGE_RESET_VECTOR_OFFSET,
                options(noreturn),
            )
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt()
}
