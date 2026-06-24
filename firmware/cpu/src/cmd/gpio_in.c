#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/gpio_in.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t gpio_in_handle(const uint8_t* payload) {
  const uint8_t flag = payload[GPIO_IN_OFFSET_FLAG];
  if (flag > GPIO_IN_FLAG_MASK) {
    return ERR_INVALID_PAYLOAD;
  }
  const uint16_t gpio_in_mask = CTL_FLAG_GPIO_IN_0 | CTL_FLAG_GPIO_IN_1 | CTL_FLAG_GPIO_IN_2 | CTL_FLAG_GPIO_IN_3;
  uint16_t ctl = fpga_read(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG);
  ctl = (uint16_t)(ctl & ~gpio_in_mask);
  if (flag & (1u << 0)) ctl |= CTL_FLAG_GPIO_IN_0;
  if (flag & (1u << 1)) ctl |= CTL_FLAG_GPIO_IN_1;
  if (flag & (1u << 2)) ctl |= CTL_FLAG_GPIO_IN_2;
  if (flag & (1u << 3)) ctl |= CTL_FLAG_GPIO_IN_3;
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
