#include "emu_glue.h"

#include <stdlib.h>

#include "app.h"
#include "proto.h"

tx_frame_t _sTx = {0};

typedef struct {
  app_state_t app;
  proto_state_t proto;
} emu_device_t;

void* emu_device_new(void) { return calloc(1u, sizeof(emu_device_t)); }

void emu_device_free(void* handle) { free(handle); }

void emu_device_select(void* handle) {
  emu_device_t* dev = (emu_device_t*)handle;
  app_set_state(&dev->app);
  proto_set_state(&dev->proto);
}

uint8_t emu_tx_ack(void) { return _sTx.ack; }
uint8_t emu_tx_data(void) { return _sTx.data; }
