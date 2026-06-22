#ifndef SRC_CMD_WRITE_MOD_H_
#define SRC_CMD_WRITE_MOD_H_

#include <stddef.h>
#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PROTO_PACKED {
  uint8_t bank;
  uint8_t _reserved;
  uint32_t offset;
  uint16_t data_len;
  uint8_t data[MOD_WRITE_MAX_DATA_LEN];
} write_mod_payload_t;

static_assert(offsetof(write_mod_payload_t, bank) == MOD_WRITE_OFFSET_BANK, "write_mod layout");
static_assert(offsetof(write_mod_payload_t, offset) == MOD_WRITE_OFFSET_OFFSET, "write_mod layout");
static_assert(offsetof(write_mod_payload_t, data_len) == MOD_WRITE_OFFSET_DATA_LEN, "write_mod layout");
static_assert(offsetof(write_mod_payload_t, data) == MOD_WRITE_OFFSET_DATA, "write_mod layout");

uint8_t write_mod_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_WRITE_MOD_H_ */
