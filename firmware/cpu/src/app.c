#ifdef __cplusplus
extern "C" {
#endif

#include "app.h"

#include <stdint.h>

#include "proto.h"

extern tx_frame_t _sTx;

#define FIFO_DEPTH (8u)
#define FIFO_MASK (FIFO_DEPTH - 1u)

static volatile uint8_t s_last_seq;
static volatile uint8_t s_last_cmd;
static volatile uint8_t s_mode;

static rx_frame_t s_fifo[FIFO_DEPTH];
static volatile uint16_t s_fifo_head;
static volatile uint16_t s_fifo_tail;

void init_app(void) {
  proto_init();
  fpga_init();
  _sTx.ack = 0xFF;
  _sTx.data = 0;
  s_last_seq = 0xFF;
  s_last_cmd = 0xFF;
  s_mode = MODE_FIFO;
  s_fifo_head = 0;
  s_fifo_tail = 0;
}

void app_set_mode(uint8_t mode) { s_mode = mode; }

uint8_t app_mode(void) { return s_mode; }

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
  if (seq == s_last_seq && cmd == s_last_cmd) return;

  if (cmd == CMD_RESET || s_mode == MODE_LOW_LATENCY) {
    if (cmd == CMD_RESET) {
      s_fifo_head = 0;
      s_fifo_tail = 0;
    }
    rx_frame_t in;
    unpack_wire(&in, frame);
    proto_handle_frame(&in, &_sTx);
    s_last_seq = seq;
    s_last_cmd = cmd;
    return;
  }

  uint16_t next = (uint16_t)((s_fifo_head + 1u) & FIFO_MASK);
  if (next == s_fifo_tail) {
    return;
  }
  unpack_wire(&s_fifo[s_fifo_head], frame);
  s_fifo_head = next;
  s_last_seq = seq;
  s_last_cmd = cmd;
}

void app_process_pending(void) {
  while (s_fifo_tail != s_fifo_head) {
    proto_handle_frame(&s_fifo[s_fifo_tail], &_sTx);
    s_fifo_tail = (uint16_t)((s_fifo_tail + 1u) & FIFO_MASK);
  }
}

#ifdef __cplusplus
}
#endif
