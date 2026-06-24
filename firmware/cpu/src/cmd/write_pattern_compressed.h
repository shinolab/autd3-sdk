#ifndef SRC_CMD_WRITE_PATTERN_COMPRESSED_H_
#define SRC_CMD_WRITE_PATTERN_COMPRESSED_H_

#include <stddef.h>
#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PROTO_PACKED {
  uint8_t bank;
  uint8_t format;
  uint8_t count;
  uint8_t _reserved;
  uint32_t offset;
  uint8_t data[NUM_TRANSDUCERS * 2];
} write_pattern_compressed_payload_t;

static_assert(offsetof(write_pattern_compressed_payload_t, bank) == EM_COMPRESSED_OFFSET_BANK,
              "write_pattern_compressed layout");
static_assert(offsetof(write_pattern_compressed_payload_t, format) == EM_COMPRESSED_OFFSET_FORMAT,
              "write_pattern_compressed layout");
static_assert(offsetof(write_pattern_compressed_payload_t, count) == EM_COMPRESSED_OFFSET_COUNT,
              "write_pattern_compressed layout");
static_assert(offsetof(write_pattern_compressed_payload_t, offset) == EM_COMPRESSED_OFFSET_OFFSET,
              "write_pattern_compressed layout");
static_assert(offsetof(write_pattern_compressed_payload_t, data) == EM_COMPRESSED_OFFSET_DATA,
              "write_pattern_compressed layout");

uint8_t write_pattern_compressed_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_WRITE_PATTERN_COMPRESSED_H_ */
