#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/silencer.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

static silencer_guard_t s_default_guard;
static silencer_guard_t* s_guard = &s_default_guard;

void silencer_set_state(silencer_guard_t* state) { s_guard = state; }

void silencer_guard_init(void) {
  uint8_t bank;
  s_guard->strict_mode = 0u;
  s_guard->completion_intensity = SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY;
  s_guard->completion_phase = SILENCER_DEFAULT_COMPLETION_STEPS_PHASE;
  for (bank = 0; bank < NUM_BANKS; bank++) {
    s_guard->mod_freq_div[bank] = 0xFFFFu;
    s_guard->pattern_freq_div[bank] = 0xFFFFu;
  }
  s_guard->mod_bank = 0u;
  s_guard->pattern_bank = 0u;
}

uint8_t silencer_violates_mod_div(uint16_t divider) {
  if (!s_guard->strict_mode) return 0u;
  return (divider < s_guard->completion_intensity) ? 1u : 0u;
}

uint8_t silencer_violates_pattern_div(uint16_t divider) {
  if (!s_guard->strict_mode) return 0u;
  return ((divider < s_guard->completion_intensity) || (divider < s_guard->completion_phase)) ? 1u : 0u;
}

uint8_t silencer_violates_mod_bank(uint8_t bank) { return silencer_violates_mod_div(s_guard->mod_freq_div[bank]); }

uint8_t silencer_violates_pattern_bank(uint8_t bank) {
  return silencer_violates_pattern_div(s_guard->pattern_freq_div[bank]);
}

void silencer_note_mod_div(uint8_t bank, uint16_t divider) { s_guard->mod_freq_div[bank] = divider; }

void silencer_note_pattern_div(uint8_t bank, uint16_t divider) { s_guard->pattern_freq_div[bank] = divider; }

void silencer_note_mod_bank(uint8_t bank) { s_guard->mod_bank = bank; }

void silencer_note_pattern_bank(uint8_t bank) { s_guard->pattern_bank = bank; }

uint8_t silencer_handle(const uint8_t* payload) {
  const silencer_payload_t* p = (const silencer_payload_t*)payload;
  if (p->flag & SILENCER_FLAG_FIXED_UPDATE_RATE_MODE) {
    if ((p->update_rate_intensity == 0u) || (p->update_rate_phase == 0u)) {
      return ERR_INVALID_PAYLOAD;
    }
    s_guard->strict_mode = 0u;
  } else {
    if ((p->completion_steps_intensity == 0u) || (p->completion_steps_phase == 0u)) {
      return ERR_INVALID_PAYLOAD;
    }
    if (p->flag & SILENCER_FLAG_STRICT_MODE) {
      const uint16_t mod_div = s_guard->mod_freq_div[s_guard->mod_bank];
      const uint16_t pattern_div = s_guard->pattern_freq_div[s_guard->pattern_bank];
      if ((mod_div < p->completion_steps_intensity) || (pattern_div < p->completion_steps_intensity) ||
          (pattern_div < p->completion_steps_phase)) {
        return ERR_INVALID_SILENCER_SETTING;
      }
      s_guard->strict_mode = 1u;
    } else {
      s_guard->strict_mode = 0u;
    }
    s_guard->completion_intensity = p->completion_steps_intensity;
    s_guard->completion_phase = p->completion_steps_phase;
  }
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_UPDATE_RATE_INTENSITY, p->update_rate_intensity);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_UPDATE_RATE_PHASE, p->update_rate_phase);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, p->completion_steps_intensity);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_COMPLETION_STEPS_PHASE, p->completion_steps_phase);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_FLAG, p->flag);
  set_and_wait_update(CTL_FLAG_SILENCER_SET);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
