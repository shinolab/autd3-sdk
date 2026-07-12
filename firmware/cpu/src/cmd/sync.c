#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/sync.h"

#include <stdint.h>

#include "app.h"
#include "fpga.h"

uint8_t sync_handle(void) {
  uint64_t next_sync0 = port_next_sync0();
  if (next_sync0 == 0u) {
    return ERR_SYNC_NOT_READY;
  }
  fpga_write_u64(ADDR_ECAT_SYNC_TIME_0, next_sync0);
  return set_and_wait_update(CTL_FLAG_SYNC_SET);
}

#ifdef __cplusplus
}
#endif
