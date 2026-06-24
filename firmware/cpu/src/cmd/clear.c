#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/clear.h"

#include <stdint.h>

#include "cmd/silencer.h"
#include "fpga.h"
#include "proto.h"

uint8_t clear_handle(void) {
  fpga_init();
  silencer_guard_init();
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
