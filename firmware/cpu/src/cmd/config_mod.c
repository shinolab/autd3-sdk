#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/config_mod.h"

#include <stdint.h>

#include "cmd/silencer.h"
#include "fpga.h"
#include "proto.h"

uint8_t config_mod_handle(const uint8_t* payload) {
  const config_mod_payload_t* p = (const config_mod_payload_t*)payload;
  if ((p->bank >= NUM_BANKS) || (p->divider == 0u) || (p->size == 0u) || (p->size > MOD_BUFFER_SAMPLES)) {
    return ERR_INVALID_PAYLOAD;
  }
  if (silencer_violates_mod_div(p->divider)) {
    return ERR_INVALID_SILENCER_SETTING;
  }
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_CYCLE0 + p->bank), (uint16_t)(p->size - 1u));
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_FREQ_DIV0 + p->bank), p->divider);
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_REP0 + p->bank), p->rep);
  silencer_note_mod_div(p->bank, p->divider);
  set_and_wait_update(CTL_FLAG_MOD_SET);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
