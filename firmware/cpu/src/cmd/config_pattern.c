#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/config_pattern.h"

#include <stdint.h>

#include "cmd/silencer.h"
#include "fpga.h"
#include "proto.h"

uint8_t config_pattern_handle(const uint8_t* payload) {
  const config_pattern_payload_t* p = (const config_pattern_payload_t*)payload;
  int invalid = (p->bank >= NUM_BANKS) || (p->type > EMISSION_TYPE_RAW) || (p->divider == 0u) || (p->size == 0u);
  if (!invalid) {
    if (p->type == EMISSION_TYPE_RAW) {
      invalid = p->size > EMISSION_MAX_INDICES;
    } else {
      invalid = (p->num_foci == 0u) || (p->num_foci > NUM_FOCI_MAX) || (p->size > MAX_FOCI_TOTAL / p->num_foci) ||
                (p->sound_speed == 0u);
    }
  }
  if (invalid) {
    return ERR_INVALID_PAYLOAD;
  }
  if (silencer_violates_pattern_div(p->divider)) {
    return ERR_INVALID_SILENCER_SETTING;
  }
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_MODE0 + p->bank), p->type);
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_CYCLE0 + p->bank), (uint16_t)(p->size - 1u));
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_FREQ_DIV0 + p->bank), p->divider);
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_SOUND_SPEED0 + p->bank), p->sound_speed);
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_NUM_FOCI0 + p->bank), p->num_foci);
  fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_REP0 + p->bank), REP_INFINITE);
  silencer_note_pattern_div(p->bank, p->divider);
  set_and_wait_update(CTL_FLAG_PATTERN_SET);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
