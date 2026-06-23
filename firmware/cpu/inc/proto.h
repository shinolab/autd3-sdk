#ifndef INC_PROTO_H_
#define INC_PROTO_H_

#include <stdint.h>

#ifndef __cplusplus
#include <assert.h>
#endif

#include "params_fpga.h"

#if defined(__GNUC__) || defined(__clang__)
#define PROTO_PACKED __attribute__((packed))
#else
#define PROTO_PACKED
#endif

#define RX_FRAME_BYTES (626)
#define PAYLOAD_BYTES (624)
#define TX_FRAME_BYTES (2)

#define WIRE_RX_FRAME_BYTES (RX_FRAME_BYTES + 2)
#define WIRE_RX_GAP_START (498)
#define WIRE_RX_GAP_END (500)

#define CMD_RESET (0x00)
#define CMD_SYNCHRONIZE (0x01)
#define CMD_SET_MODE (0x02)
#define CMD_WRITE_PATTERN_BUFFER (0x10)
#define CMD_CONFIG_PATTERN (0x11)
#define CMD_CHANGE_PATTERN_BANK (0x12)
#define CMD_WRITE_MOD_BUFFER (0x20)
#define CMD_CONFIG_MOD (0x21)
#define CMD_CHANGE_MOD_BANK (0x22)
#define CMD_SET_SILENCER (0x30)
#define CMD_READ_ERROR_DETAIL (0xE0)
#define CMD_READ_CPU_FW_VERSION_MAJOR (0xE1)
#define CMD_READ_CPU_FW_VERSION_MINOR (0xE2)
#define CMD_READ_CPU_FW_VERSION_PATCH (0xE3)
#define CMD_XOR_HASH (0xF0)

#define MOD_BUFFER_SAMPLES (65536UL)
#define EMISSION_RAM_WORDS (262144UL)
#define EMISSION_SLOT_WORDS (256)
#define FOCUS_WORDS (4)
#define MAX_FOCI_TOTAL (65536UL)

#define EM_WRITE_OFFSET_BANK (0)
#define EM_WRITE_OFFSET_OFFSET (2)
#define EM_WRITE_OFFSET_DATA_LEN (6)
#define EM_WRITE_OFFSET_DATA (8)
#define EM_WRITE_MAX_DATA_LEN (PAYLOAD_BYTES - EM_WRITE_OFFSET_DATA)

#define MOD_WRITE_OFFSET_BANK (0)
#define MOD_WRITE_OFFSET_OFFSET (2)
#define MOD_WRITE_OFFSET_DATA_LEN (6)
#define MOD_WRITE_OFFSET_DATA (8)
#define MOD_WRITE_MAX_DATA_LEN (PAYLOAD_BYTES - MOD_WRITE_OFFSET_DATA)

#define MOD_CONFIG_OFFSET_BANK (0)
#define MOD_CONFIG_OFFSET_DIVIDER (2)
#define MOD_CONFIG_OFFSET_SIZE (4)

#define EM_CONFIG_OFFSET_BANK (0)
#define EM_CONFIG_OFFSET_TYPE (1)
#define EM_CONFIG_OFFSET_DIVIDER (2)
#define EM_CONFIG_OFFSET_SIZE (4)
#define EM_CONFIG_OFFSET_NUM_FOCI (8)
#define EM_CONFIG_OFFSET_SOUND_SPEED (10)

#define CHANGE_BANK_OFFSET_BANK (0)
#define CHANGE_BANK_OFFSET_TRANSITION_MODE (1)
#define CHANGE_BANK_OFFSET_TRANSITION_VALUE (2)

#define SILENCER_OFFSET_FLAG (0)
#define SILENCER_OFFSET_UPDATE_RATE_INTENSITY (2)
#define SILENCER_OFFSET_UPDATE_RATE_PHASE (4)
#define SILENCER_OFFSET_COMPLETION_STEPS_INTENSITY (6)
#define SILENCER_OFFSET_COMPLETION_STEPS_PHASE (8)

#define SILENCER_FLAG_BIT_STRICT_MODE (1)
#define SILENCER_FLAG_STRICT_MODE (1 << SILENCER_FLAG_BIT_STRICT_MODE)

#define ERR_NONE (0x00)
#define ERR_UNKNOWN_CMD (0x01)
#define ERR_INVALID_PAYLOAD (0x02)
#define ERR_INVALID_DATA (0x03)
#define ERR_INVALID_SILENCER_SETTING (0x04)

#define MODE_FIFO (0x00)
#define MODE_LOW_LATENCY (0x01)

#define SET_MODE_OFFSET_MODE (0)

#define XOR_HASH_OFFSET_SLEEP_MS (0)
#define XOR_HASH_OFFSET_DATA_LEN (2)
#define XOR_HASH_OFFSET_DATA (4)
#define XOR_HASH_MAX_DATA_LEN (PAYLOAD_BYTES - XOR_HASH_OFFSET_DATA)

typedef struct {
  uint8_t seq;
  uint8_t cmd;
  uint8_t payload[PAYLOAD_BYTES];
} rx_frame_t;

typedef struct {
  uint16_t _reserved;
  uint8_t ack;
  uint8_t data;
} tx_frame_t;

typedef struct {
  uint8_t expected_seq;
  uint8_t ack;
  uint8_t data;
  uint8_t fw_version_major;
  uint8_t fw_version_minor;
  uint8_t fw_version_patch;
  uint8_t error_detail;
} proto_state_t;

static_assert(sizeof(rx_frame_t) == RX_FRAME_BYTES, "rx_frame_t size mismatch");
static_assert(sizeof(tx_frame_t) == 2 + TX_FRAME_BYTES, "tx_frame_t size mismatch");
static_assert(NUM_TRANSDUCERS <= EMISSION_SLOT_WORDS, "raw pattern must fit one STM slot");
static_assert(NUM_TRANSDUCERS * 2 <= EM_WRITE_MAX_DATA_LEN, "one raw emission pattern must fit one frame");

#endif /* INC_PROTO_H_ */
