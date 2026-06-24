#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/phase_corr.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t phase_corr_handle(const uint8_t* payload) {
  const uint8_t* data = &payload[PHASE_CORR_OFFSET_DATA];
  uint16_t j;
  for (j = 0; j < PHASE_CORR_WORDS; j++) {
    const uint16_t lo = data[2u * j];
    const uint16_t hi = ((uint16_t)(2u * j + 1u) < NUM_TRANSDUCERS) ? data[2u * j + 1u] : 0u;
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(((uint16_t)BRAM_CNT_SELECT_PHASE_CORR << 8) | j),
               (uint16_t)(lo | (hi << 8)));
  }
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
