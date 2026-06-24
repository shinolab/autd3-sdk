#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/pwe.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t pwe_handle(const uint8_t* payload) {
  const uint8_t* data = &payload[PWE_OFFSET_DATA];
  uint16_t i;
  for (i = 0; i < PWE_TABLE_SIZE; i++) {
    const uint16_t lo = data[2u * i];
    const uint16_t hi = data[2u * i + 1u];
    fpga_write(BRAM_SELECT_PWE_TABLE, i, (uint16_t)(lo | (hi << 8)));
  }
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
