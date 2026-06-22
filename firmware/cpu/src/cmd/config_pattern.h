#ifndef SRC_CMD_CONFIG_PATTERN_H_
#define SRC_CMD_CONFIG_PATTERN_H_

#include <stddef.h>
#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PROTO_PACKED {
  uint8_t bank;
  uint8_t type;
  uint16_t divider;
  uint32_t size;
  uint8_t num_foci;
  uint8_t _reserved;
  uint16_t sound_speed;
} config_pattern_payload_t;

static_assert(offsetof(config_pattern_payload_t, bank) == EM_CONFIG_OFFSET_BANK, "config_pattern layout");
static_assert(offsetof(config_pattern_payload_t, type) == EM_CONFIG_OFFSET_TYPE, "config_pattern layout");
static_assert(offsetof(config_pattern_payload_t, divider) == EM_CONFIG_OFFSET_DIVIDER, "config_pattern layout");
static_assert(offsetof(config_pattern_payload_t, size) == EM_CONFIG_OFFSET_SIZE, "config_pattern layout");
static_assert(offsetof(config_pattern_payload_t, num_foci) == EM_CONFIG_OFFSET_NUM_FOCI, "config_pattern layout");
static_assert(offsetof(config_pattern_payload_t, sound_speed) == EM_CONFIG_OFFSET_SOUND_SPEED, "config_pattern layout");

uint8_t config_pattern_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_CONFIG_PATTERN_H_ */
