#ifndef SRC_CMD_WRITE_PATTERN_H_
#define SRC_CMD_WRITE_PATTERN_H_

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
  uint8_t data[EM_WRITE_MAX_DATA_LEN];
} write_pattern_payload_t;

static_assert(offsetof(write_pattern_payload_t, bank) == EM_WRITE_OFFSET_BANK, "write_pattern layout");
static_assert(offsetof(write_pattern_payload_t, offset) == EM_WRITE_OFFSET_OFFSET, "write_pattern layout");
static_assert(offsetof(write_pattern_payload_t, data_len) == EM_WRITE_OFFSET_DATA_LEN, "write_pattern layout");
static_assert(offsetof(write_pattern_payload_t, data) == EM_WRITE_OFFSET_DATA, "write_pattern layout");

uint8_t write_pattern_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_WRITE_PATTERN_H_ */
