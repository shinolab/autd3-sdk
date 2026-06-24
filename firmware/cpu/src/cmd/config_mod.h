#ifndef SRC_CMD_CONFIG_MOD_H_
#define SRC_CMD_CONFIG_MOD_H_

#include <stddef.h>
#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PROTO_PACKED {
  uint8_t bank;
  uint8_t _reserved;
  uint16_t divider;
  uint32_t size;
  uint16_t rep;
} config_mod_payload_t;

static_assert(offsetof(config_mod_payload_t, bank) == MOD_CONFIG_OFFSET_BANK, "config_mod layout");
static_assert(offsetof(config_mod_payload_t, divider) == MOD_CONFIG_OFFSET_DIVIDER, "config_mod layout");
static_assert(offsetof(config_mod_payload_t, size) == MOD_CONFIG_OFFSET_SIZE, "config_mod layout");
static_assert(offsetof(config_mod_payload_t, rep) == MOD_CONFIG_OFFSET_REP, "config_mod layout");

uint8_t config_mod_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_CONFIG_MOD_H_ */
