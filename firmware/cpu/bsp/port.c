#include <stdint.h>

#include "bsp.h"
#include "rzt1_regs.h"

#define FPGA_BASE (0x44000000) /* CS1 FPGA address */

#define NANOSECONDS (1)
#define MICROSECONDS (NANOSECONDS * 1000)
#define MILLISECONDS (MICROSECONDS * 1000)

#define PORT_SYNC0_MAX_POLLS (1000000u)

void port_sleep_ms(uint16_t ms) { bsp_delay_ms(ms); }

static uint64_t read_dc_u64(volatile uint32_t* lo, volatile uint32_t* hi) {
  for (;;) {
    uint32_t low = *lo;
    uint32_t high = *hi;
    uint32_t low2 = *lo;
    if (low2 >= low) {
      return ((uint64_t)high << 32) | (uint64_t)low;
    }
  }
}

void port_fpga_write(uint16_t addr, uint16_t value) {
  volatile uint16_t* base = (volatile uint16_t*)FPGA_BASE;
  base[addr] = value;
}

uint16_t port_fpga_read(uint16_t addr) {
  volatile uint16_t* base = (volatile uint16_t*)FPGA_BASE;
  return base[addr];
}

void port_memory_barrier(void) { __asm__ volatile("dmb" ::: "memory"); }

uint64_t port_next_sync0(void) {
  uint64_t next_sync0 = read_dc_u64(&ECATC_DC_CYC_START_TIME_LO, &ECATC_DC_CYC_START_TIME_HI);
  if (next_sync0 == 0u) {
    return 0u;
  }
  uint64_t sys_time = read_dc_u64(&ECATC_DC_SYS_TIME_LO, &ECATC_DC_SYS_TIME_HI);
  uint32_t guard = 0u;
  while (next_sync0 < sys_time + 250u * MICROSECONDS) {
    if (++guard > PORT_SYNC0_MAX_POLLS) {
      return 0u;
    }
    sys_time = read_dc_u64(&ECATC_DC_SYS_TIME_LO, &ECATC_DC_SYS_TIME_HI);
    if (sys_time > next_sync0) {
      next_sync0 = read_dc_u64(&ECATC_DC_CYC_START_TIME_LO, &ECATC_DC_CYC_START_TIME_HI);
    }
  }
  return next_sync0;
}

uint64_t port_dc_sys_time(void) { return read_dc_u64(&ECATC_DC_SYS_TIME_LO, &ECATC_DC_SYS_TIME_HI); }

uint32_t port_sync0_cycle_ns(void) { return ECATC_DC_SYNC0_CYC_TIME; }
