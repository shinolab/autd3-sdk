#include "FreeRTOS.h"
#include "rzt1_regs.h"
#include "task.h"
#include <stdint.h>

#define FPGA_BASE (0x44000000) /* CS1 FPGA address */

#define NANOSECONDS (1)
#define MICROSECONDS (NANOSECONDS * 1000)
#define MILLISECONDS (MICROSECONDS * 1000)

void port_sleep_ms(uint16_t ms) {
  vTaskDelay(pdMS_TO_TICKS(ms));
}

void port_fpga_write(uint16_t addr, uint16_t value) {
  volatile uint16_t *base = (volatile uint16_t *)FPGA_BASE;
  base[addr] = value;
}

uint16_t port_fpga_read(uint16_t addr) {
  volatile uint16_t *base = (volatile uint16_t *)FPGA_BASE;
  return base[addr];
}

void port_memory_barrier(void) {
  __asm__ volatile("dmb" ::: "memory");
}

uint64_t port_next_sync0(void) {
  volatile uint64_t next_sync0 = ECATC_DC_CYC_START_TIME;
  volatile uint64_t sys_time = ECATC_DC_SYS_TIME;
  while (next_sync0 < sys_time + 250 * MICROSECONDS) {
    sys_time = ECATC_DC_SYS_TIME;
    if (sys_time > next_sync0) next_sync0 = ECATC_DC_CYC_START_TIME;
  }
  return next_sync0;
}

uint64_t port_dc_sys_time(void) {
  return ECATC_DC_SYS_TIME;
}
