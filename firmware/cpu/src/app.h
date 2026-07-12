#ifndef SRC_APP_H_
#define SRC_APP_H_

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

#define FW_VERSION_MAJOR (0)
#define FW_VERSION_MINOR (2)
#define FW_VERSION_PATCH (0)

#define FIFO_DEPTH (8u)

typedef struct {
  volatile uint8_t last_seq;
  volatile uint8_t last_cmd;
  volatile uint8_t mode;
  rx_frame_t fifo[FIFO_DEPTH];
  volatile uint16_t fifo_head;
  volatile uint16_t fifo_tail;
  volatile uint16_t fifo_flush_head;
  volatile uint16_t fifo_flush_gen;
  volatile uint16_t fifo_flush_seen;
} app_state_t;

void app_set_state(app_state_t* state);
void proto_set_state(proto_state_t* state);

void init_app(void);

void recv_ethercat(const uint8_t* frame);

void app_process_pending(void);
uint8_t app_process_one(void);

void app_set_mode(uint8_t mode);
uint8_t app_mode(void);

void proto_init(void);
void proto_set_fw_version(uint8_t major, uint8_t minor, uint8_t patch);
void proto_set_error_detail(uint8_t code);
uint8_t proto_expected_seq(void);
void proto_handle_frame(const rx_frame_t* in, volatile tx_frame_t* out);
void proto_apply_reset(volatile tx_frame_t* out);

void port_sleep_ms(uint16_t ms);

void port_fpga_write(uint16_t addr, uint16_t value);
uint16_t port_fpga_read(uint16_t addr);

void port_memory_barrier(void);

uint64_t port_next_sync0(void);

uint64_t port_dc_sys_time(void);

uint32_t port_sync0_cycle_ns(void);

#ifdef __cplusplus
}
#endif

#endif /* SRC_APP_H_ */
