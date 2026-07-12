#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/clear.h"

#include <stdint.h>

#include "cmd/silencer.h"
#include "fpga.h"
#include "proto.h"

uint8_t clear_handle(void) {
  uint8_t err = fpga_init();
  silencer_guard_init();
  return err;
}

#ifdef __cplusplus
}
#endif
