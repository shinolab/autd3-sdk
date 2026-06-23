#ifdef __cplusplus
extern "C" {
#endif

#include "app.h"

#include <stdint.h>

#include "cmd/silencer.h"
#include "proto.h"

extern tx_frame_t _sTx;

#define FIFO_MASK (FIFO_DEPTH - 1u)

static app_state_t s_default_app;
static app_state_t* s_app = &s_default_app;

void app_set_state(app_state_t* state) { s_app = state; }

void init_app(void) {
  proto_init();
  fpga_init();
  silencer_guard_init();
  _sTx.ack = 0xFF;
  _sTx.data = 0;
  s_app->last_seq = 0xFF;
  s_app->last_cmd = 0xFF;
  s_app->mode = MODE_FIFO;
  s_app->fifo_head = 0;
  s_app->fifo_tail = 0;
}

void app_set_mode(uint8_t mode) { s_app->mode = mode; }

uint8_t app_mode(void) { return s_app->mode; }

static void unpack_wire(rx_frame_t* out, const uint8_t* frame) {
  uint8_t* dst = (uint8_t*)out;
  for (uint16_t i = 0; i < WIRE_RX_GAP_START; i++) dst[i] = frame[i];
  for (uint16_t i = 0; i < RX_FRAME_BYTES - WIRE_RX_GAP_START; i++) {
    dst[WIRE_RX_GAP_START + i] = frame[WIRE_RX_GAP_END + i];
  }
}

void recv_ethercat(const uint8_t* frame) {
  uint8_t seq = frame[0];
  uint8_t cmd = frame[1];
  if (seq == s_app->last_seq && cmd == s_app->last_cmd) return;

  if (cmd == CMD_RESET || s_app->mode == MODE_LOW_LATENCY) {
    if (cmd == CMD_RESET) {
      s_app->fifo_head = 0;
      s_app->fifo_tail = 0;
    }
    rx_frame_t in;
    unpack_wire(&in, frame);
    proto_handle_frame(&in, &_sTx);
    s_app->last_seq = seq;
    s_app->last_cmd = cmd;
    return;
  }

  uint16_t next = (uint16_t)((s_app->fifo_head + 1u) & FIFO_MASK);
  if (next == s_app->fifo_tail) {
    return;
  }
  unpack_wire(&s_app->fifo[s_app->fifo_head], frame);
  s_app->fifo_head = next;
  s_app->last_seq = seq;
  s_app->last_cmd = cmd;
}

void app_process_pending(void) {
  while (s_app->fifo_tail != s_app->fifo_head) {
    proto_handle_frame(&s_app->fifo[s_app->fifo_tail], &_sTx);
    s_app->fifo_tail = (uint16_t)((s_app->fifo_tail + 1u) & FIFO_MASK);
  }
}

#ifdef __cplusplus
}
#endif
