#ifndef SRC_CMD_XOR_HASH_H_
#define SRC_CMD_XOR_HASH_H_

#include <stddef.h>
#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PROTO_PACKED {
  uint16_t sleep_ms;
  uint16_t data_len;
  uint8_t data[XOR_HASH_MAX_DATA_LEN];
} xor_hash_payload_t;

static_assert(offsetof(xor_hash_payload_t, sleep_ms) == XOR_HASH_OFFSET_SLEEP_MS, "xor_hash layout");
static_assert(offsetof(xor_hash_payload_t, data_len) == XOR_HASH_OFFSET_DATA_LEN, "xor_hash layout");
static_assert(offsetof(xor_hash_payload_t, data) == XOR_HASH_OFFSET_DATA, "xor_hash layout");

uint8_t xor_hash_handle(const uint8_t* payload);

#ifdef __cplusplus
}
#endif

#endif /* SRC_CMD_XOR_HASH_H_ */
