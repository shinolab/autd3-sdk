#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/xor_hash.h"

#include <stdint.h>

#include "app.h"
#include "proto.h"

static uint8_t xor_payload(const uint8_t* data, uint16_t len) {
  uint8_t h = 0;
  for (uint16_t i = 0; i < len; i++) h ^= data[i];
  return h;
}

uint8_t xor_hash_handle(const uint8_t* payload) {
  const xor_hash_payload_t* p = (const xor_hash_payload_t*)payload;
  if (p->data_len > XOR_HASH_MAX_DATA_LEN) {
    return ERR_INVALID_PAYLOAD;
  }
  if (p->sleep_ms != 0) port_sleep_ms(p->sleep_ms);
  if (xor_payload(p->data, p->data_len) != 0) {
    return ERR_INVALID_DATA;
  }
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
