#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/output_mask.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t output_mask_handle(const uint8_t* payload) {
  const uint8_t* data = &payload[OUTPUT_MASK_OFFSET_DATA];
  uint16_t j;
  for (j = 0; j < OUTPUT_MASK_USED_WORDS; j++) {
    const uint16_t lo = data[2u * j];
    const uint16_t hi = data[2u * j + 1u];
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(((uint16_t)BRAM_CNT_SELECT_OUTPUT_MASK << 8) | j),
               (uint16_t)(lo | (hi << 8)));
  }
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
