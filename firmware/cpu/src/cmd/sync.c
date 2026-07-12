#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/sync.h"

#include <stdint.h>

#include "app.h"
#include "fpga.h"

/* Sync0 cycle time is a multiple of 500us. The FPGA system time runs at
 * 20.48MHz, so convert ns -> ticks (x 64/3125) here and hand the FPGA the raw
 * tick count as ECAT_SYNC_CYCLE; it just accumulates it each Sync0. A 500us
 * multiple is always divisible by 3125 (500000 = 160 * 3125), so the tick
 * conversion is exact. */
#define SYNC0_CYCLE_BASE_NS (500000u)
#define SYS_TIME_NS_PER_TICK (3125u) /* 1e9 / 20.48e6 * 64 = 3125 ns per 64 ticks */

uint8_t sync_handle(void) {
  const uint32_t cycle_ns = port_sync0_cycle_ns();
  if ((cycle_ns == 0u) || ((cycle_ns % SYNC0_CYCLE_BASE_NS) != 0u)) {
    return ERR_INVALID_SYNC0_CYCLE;
  }
  const uint32_t cycle_ticks = (cycle_ns / SYS_TIME_NS_PER_TICK) * 64u;

  uint64_t next_sync0 = port_next_sync0();
  if (next_sync0 == 0u) {
    return ERR_SYNC_NOT_READY;
  }
  fpga_write_u64(ADDR_ECAT_SYNC_TIME_0, next_sync0);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_ECAT_SYNC_CYCLE_0, (uint16_t)(cycle_ticks & 0xFFFFu));
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_ECAT_SYNC_CYCLE_1, (uint16_t)(cycle_ticks >> 16));
  return set_and_wait_update(CTL_FLAG_SYNC_SET);
}

#ifdef __cplusplus
}
#endif
