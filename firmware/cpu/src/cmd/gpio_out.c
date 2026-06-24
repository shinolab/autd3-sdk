#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/gpio_out.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t gpio_out_handle(const uint8_t* payload) {
  const uint8_t* data = &payload[GPIO_OUT_OFFSET_DATA];
  const uint16_t n_words = (uint16_t)(GPIO_OUT_NUM * 4);
  uint16_t i;
  for (i = 0; i < n_words; i++) {
    const uint16_t lo = data[2u * i];
    const uint16_t hi = data[2u * i + 1u];
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_DEBUG_VALUE0_0 + i), (uint16_t)(lo | (hi << 8)));
  }
  set_and_wait_update(CTL_FLAG_DEBUG_SET);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
