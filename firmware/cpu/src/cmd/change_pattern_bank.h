#ifndef SRC_CMD_CHANGE_PATTERN_BANK_H_
#define SRC_CMD_CHANGE_PATTERN_BANK_H_

#include <stddef.h>
#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PROTO_PACKED {
  uint8_t bank;
  uint8_t transition_mode;
  uint64_t transition_value;
} change_pattern_bank_payload_t;

static_assert(offsetof(change_pattern_bank_payload_t, bank) == CHANGE_BANK_OFFSET_BANK, "change_pattern_bank layout");
static_assert(offsetof(change_pattern_bank_payload_t, transition_mode) == CHANGE_BANK_OFFSET_TRANSITION_MODE,
              "change_pattern_bank layout");
static_assert(offsetof(change_pattern_bank_payload_t, transition_value) == CHANGE_BANK_OFFSET_TRANSITION_VALUE,
              "change_pattern_bank layout");

uint8_t change_pattern_bank_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_CHANGE_PATTERN_BANK_H_ */
