#ifdef __cplusplus
extern "C" {
#endif

#include "app.h"

#include <stdint.h>
#include <string.h>

#include "cmd/silencer.h"
#include "proto.h"

extern volatile tx_frame_t _sTx;

#define FIFO_MASK (FIFO_DEPTH - 1u)
#define FIFO_CAPACITY (FIFO_DEPTH - 1u)

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
  s_app->fifo_flush_head = 0;
  s_app->fifo_flush_gen = 0;
  s_app->fifo_flush_seen = 0;
}

void app_set_mode(uint8_t mode) { s_app->mode = mode; }

uint8_t app_mode(void) { return s_app->mode; }

static void unpack_wire(rx_frame_t* out, const uint8_t* frame) {
  uint8_t* dst = (uint8_t*)out;
  memcpy(dst, frame, WIRE_RX_GAP_START);
  memcpy(dst + WIRE_RX_GAP_START, frame + WIRE_RX_GAP_END, RX_FRAME_BYTES - WIRE_RX_GAP_START);
}

void recv_ethercat(const uint8_t* frame) {
  uint8_t seq = frame[0];
  uint8_t cmd = frame[1];
  if (seq == s_app->last_seq && cmd == s_app->last_cmd) return;

  if (cmd == CMD_RESET) {
    s_app->fifo_flush_head = s_app->fifo_head;
    s_app->fifo_flush_gen = (uint16_t)(s_app->fifo_flush_gen + 1u);
  }

  uint8_t inline_ok = (cmd == CMD_RESET) || (s_app->mode == MODE_LOW_LATENCY && s_app->fifo_tail == s_app->fifo_head);
  if (inline_ok) {
    rx_frame_t in;
    unpack_wire(&in, frame);
    proto_handle_frame(&in, &_sTx);
    s_app->last_seq = seq;
    s_app->last_cmd = cmd;
    return;
  }

  uint16_t head = s_app->fifo_head;
  if ((uint16_t)(head - s_app->fifo_tail) >= FIFO_CAPACITY) {
    return;
  }
  unpack_wire(&s_app->fifo[head & FIFO_MASK], frame);
  s_app->fifo_head = (uint16_t)(head + 1u);
  s_app->last_seq = seq;
  s_app->last_cmd = cmd;
}

uint8_t app_process_one(void) {
  uint16_t gen = s_app->fifo_flush_gen;
  if (gen != s_app->fifo_flush_seen) {
    s_app->fifo_flush_seen = gen;
    uint16_t flush_head = s_app->fifo_flush_head;
    if ((uint16_t)(flush_head - s_app->fifo_tail) < FIFO_DEPTH) {
      s_app->fifo_tail = flush_head;
    }
  }
  if (s_app->fifo_tail == s_app->fifo_head) {
    return 0u;
  }
  proto_handle_frame(&s_app->fifo[s_app->fifo_tail & FIFO_MASK], &_sTx);
  s_app->fifo_tail = (uint16_t)(s_app->fifo_tail + 1u);
  if (s_app->fifo_flush_gen != gen) {
    proto_apply_reset(&_sTx);
  }
  return 1u;
}

void app_process_pending(void) {
  while (app_process_one() != 0u) {
  }
}

#ifdef __cplusplus
}
#endif
