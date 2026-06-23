#ifndef SRC_CMD_SILENCER_H_
#define SRC_CMD_SILENCER_H_

#include <stddef.h>
#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
  uint8_t strict_mode;
  uint16_t completion_intensity;
  uint16_t completion_phase;
  uint16_t mod_freq_div[NUM_BANKS];
  uint16_t pattern_freq_div[NUM_BANKS];
  uint8_t mod_bank;
  uint8_t pattern_bank;
} silencer_guard_t;

void silencer_set_state(silencer_guard_t* state);
void silencer_guard_init(void);

uint8_t silencer_violates_mod_div(uint16_t divider);
uint8_t silencer_violates_pattern_div(uint16_t divider);
uint8_t silencer_violates_mod_bank(uint8_t bank);
uint8_t silencer_violates_pattern_bank(uint8_t bank);
void silencer_note_mod_div(uint8_t bank, uint16_t divider);
void silencer_note_pattern_div(uint8_t bank, uint16_t divider);
void silencer_note_mod_bank(uint8_t bank);
void silencer_note_pattern_bank(uint8_t bank);

typedef struct PROTO_PACKED {
  uint8_t flag;
  uint8_t _reserved;
  uint16_t update_rate_intensity;
  uint16_t update_rate_phase;
  uint16_t completion_steps_intensity;
  uint16_t completion_steps_phase;
} silencer_payload_t;

static_assert(offsetof(silencer_payload_t, flag) == SILENCER_OFFSET_FLAG, "silencer layout");
static_assert(offsetof(silencer_payload_t, update_rate_intensity) == SILENCER_OFFSET_UPDATE_RATE_INTENSITY,
              "silencer layout");
static_assert(offsetof(silencer_payload_t, update_rate_phase) == SILENCER_OFFSET_UPDATE_RATE_PHASE, "silencer layout");
static_assert(offsetof(silencer_payload_t, completion_steps_intensity) == SILENCER_OFFSET_COMPLETION_STEPS_INTENSITY,
              "silencer layout");
static_assert(offsetof(silencer_payload_t, completion_steps_phase) == SILENCER_OFFSET_COMPLETION_STEPS_PHASE,
              "silencer layout");

uint8_t silencer_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_SILENCER_H_ */
