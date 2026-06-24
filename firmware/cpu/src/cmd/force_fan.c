#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/force_fan.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t force_fan_handle(const uint8_t* payload) {
  const uint8_t value = payload[FORCE_FAN_OFFSET_VALUE];
  if (value > 1u) {
    return ERR_INVALID_PAYLOAD;
  }
  uint16_t ctl = fpga_read(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG);
  if (value != 0u) {
    ctl |= CTL_FLAG_FORCE_FAN;
  } else {
    ctl = (uint16_t)(ctl & ~CTL_FLAG_FORCE_FAN);
  }
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
