#ifdef __cplusplus
extern "C" {
#endif

#include "proto.h"

#include <stdint.h>

#include "app.h"
#include "cmd/change_mod_bank.h"
#include "cmd/change_pattern_bank.h"
#include "cmd/config_mod.h"
#include "cmd/config_pattern.h"
#include "cmd/set_mode.h"
#include "cmd/silencer.h"
#include "cmd/sync.h"
#include "cmd/write_mod.h"
#include "cmd/write_pattern.h"
#include "cmd/xor_hash.h"

static proto_state_t s_default_proto;
static proto_state_t* s_proto = &s_default_proto;

void proto_set_state(proto_state_t* state) { s_proto = state; }

void proto_init(void) {
  s_proto->expected_seq = 0;
  s_proto->ack = 0xFF;
  s_proto->data = 0;
  s_proto->error_detail = ERR_NONE;
  proto_set_fw_version(FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH);
}

void proto_set_fw_version(uint8_t major, uint8_t minor, uint8_t patch) {
  s_proto->fw_version_major = major;
  s_proto->fw_version_minor = minor;
  s_proto->fw_version_patch = patch;
}

void proto_set_error_detail(uint8_t code) { s_proto->error_detail = code; }

uint8_t proto_expected_seq(void) { return s_proto->expected_seq; }

static uint8_t latch_error(uint8_t data) {
  if (data != ERR_NONE) s_proto->error_detail = data;
  return data;
}

static uint8_t dispatch(const rx_frame_t* in) {
  switch (in->cmd) {
    case CMD_XOR_HASH:
      return latch_error(xor_hash_handle(in->payload));
    case CMD_READ_CPU_FW_VERSION_MAJOR:
      return s_proto->fw_version_major;
    case CMD_READ_CPU_FW_VERSION_MINOR:
      return s_proto->fw_version_minor;
    case CMD_READ_CPU_FW_VERSION_PATCH:
      return s_proto->fw_version_patch;
    case CMD_READ_ERROR_DETAIL:
      return s_proto->error_detail;
    case CMD_WRITE_PATTERN_BUFFER:
      return latch_error(write_pattern_handle(in->payload));
    case CMD_WRITE_MOD_BUFFER:
      return latch_error(write_mod_handle(in->payload));
    case CMD_CONFIG_MOD:
      return latch_error(config_mod_handle(in->payload));
    case CMD_CONFIG_PATTERN:
      return latch_error(config_pattern_handle(in->payload));
    case CMD_CHANGE_MOD_BANK:
      return latch_error(change_mod_bank_handle(in->payload));
    case CMD_CHANGE_PATTERN_BANK:
      return latch_error(change_pattern_bank_handle(in->payload));
    case CMD_SET_SILENCER:
      return latch_error(silencer_handle(in->payload));
    case CMD_SYNCHRONIZE:
      return latch_error(sync_handle());
    case CMD_SET_MODE:
      return latch_error(set_mode_handle(in->payload));
    default:
      return latch_error(ERR_UNKNOWN_CMD);
  }
}

void proto_handle_frame(const rx_frame_t* in, tx_frame_t* out) {
  if (in->cmd == CMD_RESET) {
    s_proto->expected_seq = 0;
    s_proto->ack = 0xFF;
    s_proto->data = 0;
  } else if (in->seq == s_proto->expected_seq) {
    s_proto->ack = in->seq;
    s_proto->expected_seq = (uint8_t)(s_proto->expected_seq + 1u);
    s_proto->data = dispatch(in);
  }

  out->ack = s_proto->ack;
  out->data = s_proto->data;
}

#ifdef __cplusplus
}
#endif
