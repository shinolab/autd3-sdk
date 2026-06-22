#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/sync.h"

#include <stdint.h>

#include "app.h"
#include "fpga.h"

uint8_t sync_handle(void) {
  fpga_write_u64(ADDR_ECAT_SYNC_TIME_0, port_next_sync0());
  set_and_wait_update(CTL_FLAG_SYNC_SET);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
