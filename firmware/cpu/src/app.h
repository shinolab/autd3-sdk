#ifndef SRC_APP_H_
#define SRC_APP_H_

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef int bool_t;
#ifndef true
#define true (1)
#endif
#ifndef false
#define false (0)
#endif

#define FW_VERSION_MAJOR (0)
#define FW_VERSION_MINOR (1)
#define FW_VERSION_PATCH (0)

#define FIFO_DEPTH (8u)

typedef struct {
  volatile uint8_t last_seq;
  volatile uint8_t last_cmd;
  volatile uint8_t mode;
  rx_frame_t fifo[FIFO_DEPTH];
  volatile uint16_t fifo_head;
  volatile uint16_t fifo_tail;
} app_state_t;

void app_set_state(app_state_t* state);
void proto_set_state(proto_state_t* state);

void init_app(void);

void recv_ethercat(const uint8_t* frame);

void app_process_pending(void);

void app_set_mode(uint8_t mode);
uint8_t app_mode(void);

void proto_init(void);
void proto_set_fw_version(uint8_t major, uint8_t minor, uint8_t patch);
void proto_set_error_detail(uint8_t code);
uint8_t proto_expected_seq(void);
void proto_handle_frame(const rx_frame_t* in, tx_frame_t* out);

void port_sleep_ms(uint16_t ms);

void port_fpga_write(uint16_t addr, uint16_t value);
uint16_t port_fpga_read(uint16_t addr);

uint64_t port_next_sync0(void);

uint64_t port_dc_sys_time(void);

#ifdef __cplusplus
}
#endif

#endif /* SRC_APP_H_ */
