#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/change_pattern_bank.h"

#include <stdint.h>

#include "app.h"
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
  if (transition_mode_violates_loop(fpga_read(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_REP0 + p->bank)),
                                    p->transition_mode)) {
    return ERR_INVALID_TRANSITION_MODE;
  }
  if ((p->transition_mode == TRANSITION_MODE_SYS_TIME) &&
      (p->transition_value < port_dc_sys_time() + SYS_TIME_TRANSITION_MARGIN_NS)) {
    return ERR_MISS_TRANSITION_TIME;
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
