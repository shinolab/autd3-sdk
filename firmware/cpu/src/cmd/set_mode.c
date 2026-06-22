#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/set_mode.h"

#include <stdint.h>

#include "app.h"
#include "proto.h"

uint8_t set_mode_handle(const uint8_t* payload) {
  uint8_t mode = payload[SET_MODE_OFFSET_MODE];
  if (mode > MODE_LOW_LATENCY) return ERR_INVALID_PAYLOAD;
  app_set_mode(mode);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
