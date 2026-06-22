#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/config_mod.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t config_mod_handle(const uint8_t* payload) {
  const config_mod_payload_t* p = (const config_mod_payload_t*)payload;
  if ((p->bank >= NUM_BANKS) || (p->divider == 0u) || (p->size == 0u) || (p->size > MOD_BUFFER_SAMPLES)) {
    return ERR_INVALID_PAYLOAD;
  }
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_CYCLE0 + p->bank), (uint16_t)(p->size - 1u));
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_FREQ_DIV0 + p->bank), p->divider);
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_REP0 + p->bank), REP_INFINITE);
  set_and_wait_update(CTL_FLAG_MOD_SET);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
