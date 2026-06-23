#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/change_pattern_bank.h"

#include <stdint.h>

#include "cmd/silencer.h"
#include "fpga.h"
#include "proto.h"

uint8_t change_pattern_bank_handle(const uint8_t* payload) {
  const change_pattern_bank_payload_t* p = (const change_pattern_bank_payload_t*)payload;
  if (p->bank >= NUM_BANKS) {
    return ERR_INVALID_PAYLOAD;
  }
  if (silencer_violates_pattern_bank(p->bank)) {
    return ERR_INVALID_SILENCER_SETTING;
  }
  fpga_write_change_bank(ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE, ADDR_PATTERN_TRANSITION_VALUE_0,
                         p->bank, p->transition_mode, p->transition_value);
  silencer_note_pattern_bank(p->bank);
  set_and_wait_update(CTL_FLAG_PATTERN_SET);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
