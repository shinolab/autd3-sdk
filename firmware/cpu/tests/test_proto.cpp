#include <gtest/gtest.h>

#include <algorithm>
#include <cstring>
#include <vector>

extern "C" {
#include "app.h"
#include "proto.h"

extern tx_frame_t _sTx;
uint32_t port_test_total_sleep_ms();
void port_test_reset_sleep();
void port_test_fpga_reset();
void port_test_set_next_sync0(uint64_t t);
uint16_t port_test_fpga_ctl(uint16_t addr);
uint16_t port_test_fpga_phase_corr(uint16_t idx);
uint16_t port_test_fpga_output_mask(uint16_t idx);
uint16_t port_test_fpga_pwe_word(uint16_t idx);
uint32_t port_test_fpga_latch_count(uint16_t flag);
uint16_t port_test_fpga_mod_word(uint8_t bank, uint32_t word_idx);
uint16_t port_test_fpga_emission_word(uint8_t bank, uint32_t word_idx);
void port_test_fpga_set_controller(uint16_t addr, uint16_t value);
}

namespace {

class Frame {
 public:
  Frame(uint8_t seq, uint8_t cmd) {
    std::memset(&rx_, 0, sizeof(rx_));
    rx_.seq = seq;
    rx_.cmd = cmd;
  }

  uint8_t* payload() { return rx_.payload; }

  void deliver() const {
    deliver_no_drain();
    app_process_pending();
  }

  void deliver_no_drain() const {
    uint8_t wire[WIRE_RX_FRAME_BYTES];
    const uint8_t* logical = reinterpret_cast<const uint8_t*>(&rx_);
    std::memcpy(wire, logical, WIRE_RX_GAP_START);
    std::memset(wire + WIRE_RX_GAP_START, 0, WIRE_RX_GAP_END - WIRE_RX_GAP_START);
    std::memcpy(wire + WIRE_RX_GAP_END, logical + WIRE_RX_GAP_START, RX_FRAME_BYTES - WIRE_RX_GAP_START);
    recv_ethercat(wire);
  }

 private:
  rx_frame_t rx_{};
};

Frame make_xor_hash_ok(uint8_t seq, uint16_t sleep_ms, const std::vector<uint8_t>& data) {
  Frame f(seq, CMD_XOR_HASH);
  uint8_t* p = f.payload();
  p[XOR_HASH_OFFSET_SLEEP_MS] = static_cast<uint8_t>(sleep_ms & 0xFF);
  p[XOR_HASH_OFFSET_SLEEP_MS + 1] = static_cast<uint8_t>((sleep_ms >> 8) & 0xFF);

  uint8_t checksum = 0;
  for (auto b : data) checksum ^= b;

  uint16_t len = static_cast<uint16_t>(data.size() + 1);
  p[XOR_HASH_OFFSET_DATA_LEN] = static_cast<uint8_t>(len & 0xFF);
  p[XOR_HASH_OFFSET_DATA_LEN + 1] = static_cast<uint8_t>((len >> 8) & 0xFF);
  std::memcpy(p + XOR_HASH_OFFSET_DATA, data.data(), data.size());
  p[XOR_HASH_OFFSET_DATA + data.size()] = checksum;
  return f;
}

Frame make_xor_hash_bad(uint8_t seq, const std::vector<uint8_t>& data) {
  Frame f(seq, CMD_XOR_HASH);
  uint8_t* p = f.payload();
  p[XOR_HASH_OFFSET_SLEEP_MS] = 0;
  p[XOR_HASH_OFFSET_SLEEP_MS + 1] = 0;
  uint16_t len = static_cast<uint16_t>(data.size());
  p[XOR_HASH_OFFSET_DATA_LEN] = static_cast<uint8_t>(len & 0xFF);
  p[XOR_HASH_OFFSET_DATA_LEN + 1] = static_cast<uint8_t>((len >> 8) & 0xFF);
  std::memcpy(p + XOR_HASH_OFFSET_DATA, data.data(), data.size());
  return f;
}

void put_u16_le(uint8_t* p, uint16_t v) {
  p[0] = static_cast<uint8_t>(v & 0xFF);
  p[1] = static_cast<uint8_t>((v >> 8) & 0xFF);
}

void put_u32_le(uint8_t* p, uint32_t v) {
  p[0] = static_cast<uint8_t>(v & 0xFF);
  p[1] = static_cast<uint8_t>((v >> 8) & 0xFF);
  p[2] = static_cast<uint8_t>((v >> 16) & 0xFF);
  p[3] = static_cast<uint8_t>((v >> 24) & 0xFF);
}

void put_u64_le(uint8_t* p, uint64_t v) {
  for (int i = 0; i < 8; ++i) {
    p[i] = static_cast<uint8_t>((v >> (8 * i)) & 0xFF);
  }
}

void reset_all() {
  port_test_fpga_reset();
  init_app();
}

Frame make_write_pattern_buffer(uint8_t seq, uint8_t bank, uint32_t offset_words, const std::vector<uint16_t>& words) {
  Frame f(seq, CMD_WRITE_PATTERN_BUFFER);
  uint8_t* p = f.payload();
  p[EM_WRITE_OFFSET_BANK] = bank;
  put_u32_le(p + EM_WRITE_OFFSET_OFFSET, offset_words);
  put_u16_le(p + EM_WRITE_OFFSET_DATA_LEN, static_cast<uint16_t>(words.size() * 2));
  for (size_t i = 0; i < words.size(); ++i) {
    put_u16_le(p + EM_WRITE_OFFSET_DATA + 2 * i, words[i]);
  }
  return f;
}

Frame make_write_pattern_compressed(uint8_t seq, uint8_t bank, uint32_t offset_words, uint8_t format, uint8_t count,
                                    const std::vector<uint16_t>& words) {
  Frame f(seq, CMD_WRITE_PATTERN_COMPRESSED);
  uint8_t* p = f.payload();
  p[EM_COMPRESSED_OFFSET_BANK] = bank;
  p[EM_COMPRESSED_OFFSET_FORMAT] = format;
  p[EM_COMPRESSED_OFFSET_COUNT] = count;
  put_u32_le(p + EM_COMPRESSED_OFFSET_OFFSET, offset_words);
  for (size_t i = 0; i < words.size(); ++i) {
    put_u16_le(p + EM_COMPRESSED_OFFSET_DATA + 2 * i, words[i]);
  }
  return f;
}

Frame make_write_mod_buffer(uint8_t seq, uint8_t bank, uint32_t offset, const std::vector<uint8_t>& data) {
  Frame f(seq, CMD_WRITE_MOD_BUFFER);
  uint8_t* p = f.payload();
  p[MOD_WRITE_OFFSET_BANK] = bank;
  put_u32_le(p + MOD_WRITE_OFFSET_OFFSET, offset);
  put_u16_le(p + MOD_WRITE_OFFSET_DATA_LEN, static_cast<uint16_t>(data.size()));
  std::memcpy(p + MOD_WRITE_OFFSET_DATA, data.data(), data.size());
  return f;
}

Frame make_config_mod(uint8_t seq, uint8_t bank, uint16_t divider, uint32_t size, uint16_t rep = REP_INFINITE) {
  Frame f(seq, CMD_CONFIG_MOD);
  uint8_t* p = f.payload();
  p[MOD_CONFIG_OFFSET_BANK] = bank;
  put_u16_le(p + MOD_CONFIG_OFFSET_DIVIDER, divider);
  put_u32_le(p + MOD_CONFIG_OFFSET_SIZE, size);
  put_u16_le(p + MOD_CONFIG_OFFSET_REP, rep);
  return f;
}

Frame make_config_pattern(uint8_t seq, uint8_t bank, uint8_t type, uint16_t divider, uint32_t size, uint8_t num_foci,
                          uint16_t sound_speed, uint16_t rep = REP_INFINITE) {
  Frame f(seq, CMD_CONFIG_PATTERN);
  uint8_t* p = f.payload();
  p[EM_CONFIG_OFFSET_BANK] = bank;
  p[EM_CONFIG_OFFSET_TYPE] = type;
  put_u16_le(p + EM_CONFIG_OFFSET_DIVIDER, divider);
  put_u32_le(p + EM_CONFIG_OFFSET_SIZE, size);
  p[EM_CONFIG_OFFSET_NUM_FOCI] = num_foci;
  put_u16_le(p + EM_CONFIG_OFFSET_SOUND_SPEED, sound_speed);
  put_u16_le(p + EM_CONFIG_OFFSET_REP, rep);
  return f;
}

Frame make_change_pattern_bank(uint8_t seq, uint8_t bank, uint8_t transition_mode, uint64_t transition_value) {
  Frame f(seq, CMD_CHANGE_PATTERN_BANK);
  uint8_t* p = f.payload();
  p[CHANGE_BANK_OFFSET_BANK] = bank;
  p[CHANGE_BANK_OFFSET_TRANSITION_MODE] = transition_mode;
  put_u64_le(p + CHANGE_BANK_OFFSET_TRANSITION_VALUE, transition_value);
  return f;
}

Frame make_change_mod_bank(uint8_t seq, uint8_t bank, uint8_t transition_mode, uint64_t transition_value) {
  Frame f(seq, CMD_CHANGE_MOD_BANK);
  uint8_t* p = f.payload();
  p[CHANGE_BANK_OFFSET_BANK] = bank;
  p[CHANGE_BANK_OFFSET_TRANSITION_MODE] = transition_mode;
  put_u64_le(p + CHANGE_BANK_OFFSET_TRANSITION_VALUE, transition_value);
  return f;
}

Frame make_set_silencer(uint8_t seq, uint8_t flag, uint16_t update_rate_intensity, uint16_t update_rate_phase,
                        uint16_t completion_steps_intensity, uint16_t completion_steps_phase) {
  Frame f(seq, CMD_SET_SILENCER);
  uint8_t* p = f.payload();
  p[SILENCER_OFFSET_FLAG] = flag;
  put_u16_le(p + SILENCER_OFFSET_UPDATE_RATE_INTENSITY, update_rate_intensity);
  put_u16_le(p + SILENCER_OFFSET_UPDATE_RATE_PHASE, update_rate_phase);
  put_u16_le(p + SILENCER_OFFSET_COMPLETION_STEPS_INTENSITY, completion_steps_intensity);
  put_u16_le(p + SILENCER_OFFSET_COMPLETION_STEPS_PHASE, completion_steps_phase);
  return f;
}

Frame make_force_fan(uint8_t seq, uint8_t value) {
  Frame f(seq, CMD_FORCE_FAN);
  f.payload()[FORCE_FAN_OFFSET_VALUE] = value;
  return f;
}

Frame make_gpio_in(uint8_t seq, uint8_t flag) {
  Frame f(seq, CMD_EMULATE_GPIO_IN);
  f.payload()[GPIO_IN_OFFSET_FLAG] = flag;
  return f;
}

Frame make_phase_corr(uint8_t seq, const std::vector<uint8_t>& phases) {
  Frame f(seq, CMD_SET_PHASE_CORR);
  std::memcpy(f.payload() + PHASE_CORR_OFFSET_DATA, phases.data(), phases.size());
  return f;
}

Frame make_output_mask(uint8_t seq, const std::vector<uint16_t>& words) {
  Frame f(seq, CMD_SET_OUTPUT_MASK);
  for (size_t i = 0; i < words.size(); ++i) {
    put_u16_le(f.payload() + OUTPUT_MASK_OFFSET_DATA + 2 * i, words[i]);
  }
  return f;
}

Frame make_pwe(uint8_t seq, const std::vector<uint16_t>& table) {
  Frame f(seq, CMD_SET_PWE);
  for (size_t i = 0; i < table.size(); ++i) {
    put_u16_le(f.payload() + PWE_OFFSET_DATA + 2 * i, table[i]);
  }
  return f;
}

Frame make_gpio_out(uint8_t seq, const std::vector<uint64_t>& values) {
  Frame f(seq, CMD_SET_GPIO_OUT);
  for (size_t i = 0; i < values.size(); ++i) {
    put_u64_le(f.payload() + GPIO_OUT_OFFSET_DATA + 8 * i, values[i]);
  }
  return f;
}

}

TEST(Proto, InitialAckIsSentinelByte) {
  init_app();
  EXPECT_EQ(_sTx.ack, 0xFF);
  EXPECT_EQ(proto_expected_seq(), 0);
}

TEST(Proto, MatchingSeqAdvancesAckAndExpectedSeqForAllNonResetCmds) {
  init_app();
  proto_set_fw_version(0xAB, 0x12, 0x34);
  proto_set_error_detail(0xCD);

  make_xor_hash_ok(0, 0, {0x01, 0x02, 0x04}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(proto_expected_seq(), 1);
  EXPECT_EQ(_sTx.data, 0);

  Frame(1, CMD_READ_CPU_FW_VERSION_MAJOR).deliver();
  EXPECT_EQ(_sTx.ack, 1);
  EXPECT_EQ(proto_expected_seq(), 2);
  EXPECT_EQ(_sTx.data, 0xAB);

  Frame(2, CMD_READ_CPU_FW_VERSION_MINOR).deliver();
  EXPECT_EQ(_sTx.ack, 2);
  EXPECT_EQ(proto_expected_seq(), 3);
  EXPECT_EQ(_sTx.data, 0x12);

  Frame(3, CMD_READ_CPU_FW_VERSION_PATCH).deliver();
  EXPECT_EQ(_sTx.ack, 3);
  EXPECT_EQ(proto_expected_seq(), 4);
  EXPECT_EQ(_sTx.data, 0x34);

  Frame(4, CMD_READ_ERROR_DETAIL).deliver();
  EXPECT_EQ(_sTx.ack, 4);
  EXPECT_EQ(proto_expected_seq(), 5);
  EXPECT_EQ(_sTx.data, 0xCD);
}

TEST(Proto, NopAcksWithoutChangingState) {
  init_app();
  proto_set_error_detail(0xCD);

  Frame(0, CMD_NOP).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(proto_expected_seq(), 1);

  Frame(1, CMD_READ_ERROR_DETAIL).deliver();
  EXPECT_EQ(_sTx.data, 0xCD) << "Nop must not touch error_detail";
}

TEST(Proto, MismatchedSeqIsDropped) {
  init_app();

  make_xor_hash_ok(5, 0, {0xAA}).deliver();

  EXPECT_EQ(_sTx.ack, 0xFF) << "ack must not advance on dropped frame";
  EXPECT_EQ(proto_expected_seq(), 0);
}

TEST(Proto, SeqWraparoundBoundary) {
  init_app();
  for (uint16_t i = 0; i < 257; ++i) {
    make_xor_hash_ok(static_cast<uint8_t>(i & 0xFF), 0, {}).deliver();
  }
  EXPECT_EQ(proto_expected_seq(), 1) << "257 mod 256 = 1";
  EXPECT_EQ(_sTx.ack, 0) << "last accepted SEQ before wrap was 0";
}

TEST(Proto, UnknownStreamingCmdSetsErrorDetailAndReturnsErrorInData) {
  init_app();
  Frame(0, 0x7F ).deliver();
  EXPECT_EQ(_sTx.data, ERR_UNKNOWN_CMD);
  Frame(1, CMD_READ_ERROR_DETAIL).deliver();
  EXPECT_EQ(_sTx.data, ERR_UNKNOWN_CMD);
}

TEST(Proto, UnknownNonStreamingCmdSetsErrorDetail) {
  init_app();
  Frame(0, 0xEE ).deliver();
  EXPECT_EQ(_sTx.data, ERR_UNKNOWN_CMD);
}


TEST(Proto, XorHashWithXorZeroReturnsSuccess) {
  init_app();
  port_test_reset_sleep();
  make_xor_hash_ok(0, 0, {0x11, 0x22, 0x33}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(proto_expected_seq(), 1);
}

TEST(Proto, XorHashWithNonZeroXorReturnsErrInvalidData) {
  init_app();
  make_xor_hash_bad(0, {0xAA}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, ERR_INVALID_DATA);
  Frame(1, CMD_READ_ERROR_DETAIL).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_DATA);
}

TEST(Proto, XorHashSleepIsForwardedToPortHook) {
  init_app();
  port_test_reset_sleep();
  make_xor_hash_ok(0, 7, {0x01}).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_total_sleep_ms(), 7u);
}

TEST(Proto, XorHashTooLargeDataLenReturnsErrInvalidPayload) {
  init_app();

  Frame f(0, CMD_XOR_HASH);
  uint8_t* p = f.payload();
  p[XOR_HASH_OFFSET_SLEEP_MS] = 0;
  p[XOR_HASH_OFFSET_SLEEP_MS + 1] = 0;
  uint16_t bad_len = XOR_HASH_MAX_DATA_LEN + 1;
  p[XOR_HASH_OFFSET_DATA_LEN] = static_cast<uint8_t>(bad_len & 0xFF);
  p[XOR_HASH_OFFSET_DATA_LEN + 1] = static_cast<uint8_t>((bad_len >> 8) & 0xFF);
  f.deliver();

  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
}

TEST(Proto, XorHashEmptyDataReturnsSuccess) {
  init_app();
  make_xor_hash_ok(0, 0, {}).deliver();
  EXPECT_EQ(_sTx.data, 0);
}


TEST(Proto, ConsecutiveFramesEachProcessImmediately) {
  init_app();

  make_xor_hash_ok(0, 0, {}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  make_xor_hash_ok(1, 0, {}).deliver();
  EXPECT_EQ(_sTx.ack, 1);
  make_xor_hash_ok(2, 0, {}).deliver();
  EXPECT_EQ(_sTx.ack, 2);
  EXPECT_EQ(proto_expected_seq(), 3);
}

TEST(Proto, DuplicateFrameIsSuppressedAtIsrBoundary) {
  init_app();
  Frame f = make_xor_hash_ok(0, 0, {0x42});
  f.deliver();
  f.deliver(); 
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(proto_expected_seq(), 1) << "would be 2 if duplicate slipped through";
}

TEST(Proto, SameSeqDifferentCmdIsNotSuppressedAtIsrBoundary) {
  init_app();
  Frame(0, CMD_RESET).deliver();
  ASSERT_EQ(proto_expected_seq(), 0);

  make_xor_hash_ok(0, 0, {0xCD}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(proto_expected_seq(), 1);
}

TEST(Proto, DedupStateResetsOnInitApp) {
  init_app();
  make_xor_hash_ok(0, 0, {}).deliver();
  ASSERT_EQ(proto_expected_seq(), 1);

  init_app();
  make_xor_hash_ok(0, 0, {}).deliver();
  EXPECT_EQ(proto_expected_seq(), 1) << "first frame after re-init must not be deduped";
}

TEST(Proto, ResetReturnsProtoStateToPostBootBaseline) {
  init_app();
  proto_set_fw_version(0x42, 0x05, 0x99);
  proto_set_error_detail(0x33);

  make_xor_hash_ok(0, 0, {}).deliver();
  make_xor_hash_ok(1, 0, {}).deliver();
  ASSERT_EQ(_sTx.ack, 1);
  ASSERT_EQ(proto_expected_seq(), 2);

  Frame(99 , CMD_RESET).deliver();
  EXPECT_EQ(_sTx.ack, 0xFF);
  EXPECT_EQ(proto_expected_seq(), 0);

  Frame(0, CMD_READ_CPU_FW_VERSION_MAJOR).deliver();
  EXPECT_EQ(_sTx.data, 0x42);
  Frame(1, CMD_READ_CPU_FW_VERSION_MINOR).deliver();
  EXPECT_EQ(_sTx.data, 0x05);
  Frame(2, CMD_READ_CPU_FW_VERSION_PATCH).deliver();
  EXPECT_EQ(_sTx.data, 0x99);
  Frame(3, CMD_READ_ERROR_DETAIL).deliver();
  EXPECT_EQ(_sTx.data, 0x33);
}

TEST(Proto, HandshakeSurvivesWorstCaseDedupCollisionAfterCrashedClient) {
  init_app();
  Frame(0, CMD_RESET).deliver();

  make_xor_hash_ok(0, 0, {}).deliver();
  ASSERT_EQ(proto_expected_seq(), 1);

  Frame(0, CMD_RESET).deliver();
  Frame(1, CMD_RESET).deliver();

  EXPECT_EQ(_sTx.ack, 0xFF);
  EXPECT_EQ(proto_expected_seq(), 0);

  make_xor_hash_ok(0, 0, {0x11, 0x22}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(proto_expected_seq(), 1);
}

TEST(Proto, DefaultModeIsFifo) {
  init_app();
  EXPECT_EQ(app_mode(), MODE_FIFO);
}

TEST(Proto, FifoModeDefersProcessingUntilDrained) {
  init_app();

  make_xor_hash_ok(0, 0, {}).deliver_no_drain();
  EXPECT_EQ(_sTx.ack, 0xFF) << "frame must not be processed before drain in FIFO mode";
  EXPECT_EQ(proto_expected_seq(), 0);

  app_process_pending();
  EXPECT_EQ(_sTx.ack, 0) << "drain processes the queued frame";
  EXPECT_EQ(proto_expected_seq(), 1);
}

TEST(Proto, FifoModeDrainsInOrder) {
  init_app();

  make_xor_hash_ok(0, 0, {}).deliver_no_drain();
  make_xor_hash_ok(1, 0, {}).deliver_no_drain();
  make_xor_hash_ok(2, 0, {}).deliver_no_drain();
  EXPECT_EQ(proto_expected_seq(), 0) << "nothing processed yet";

  app_process_pending();
  EXPECT_EQ(_sTx.ack, 2) << "all three drained in SEQ order";
  EXPECT_EQ(proto_expected_seq(), 3);
}

Frame make_set_mode(uint8_t seq, uint8_t mode) {
  Frame f(seq, CMD_SET_MODE);
  f.payload()[SET_MODE_OFFSET_MODE] = mode;
  return f;
}

TEST(Proto, SetModeLowLatencyProcessesFramesInline) {
  init_app();

  make_set_mode(0, MODE_LOW_LATENCY).deliver();
  EXPECT_EQ(app_mode(), MODE_LOW_LATENCY);
  EXPECT_EQ(_sTx.ack, 0);

  make_xor_hash_ok(1, 0, {}).deliver_no_drain();
  EXPECT_EQ(_sTx.ack, 1) << "low-latency mode processes inline";
  EXPECT_EQ(proto_expected_seq(), 2);
}

TEST(Proto, SetModeRejectsUnknownMode) {
  init_app();

  make_set_mode(0, 0x02).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  EXPECT_EQ(app_mode(), MODE_FIFO) << "mode must stay at default on invalid payload";
}

TEST(Proto, ResetIsProcessedInlineAndFlushesQueueInFifoMode) {
  init_app();

  make_xor_hash_ok(0, 0, {}).deliver_no_drain();
  EXPECT_EQ(proto_expected_seq(), 0);

  Frame(0, CMD_RESET).deliver_no_drain();
  EXPECT_EQ(_sTx.ack, 0xFF);
  EXPECT_EQ(proto_expected_seq(), 0);

  app_process_pending();
  EXPECT_EQ(_sTx.ack, 0xFF) << "flushed frame must not surface after Reset";
  EXPECT_EQ(proto_expected_seq(), 0);
}

TEST(Proto, WritePatternBufferWritesWordsAtOffsetPerBank) {
  reset_all();

  make_write_pattern_buffer(0, 0, 0, {0x1234, 0x5678}).deliver();
  EXPECT_EQ(_sTx.data, 0);
  make_write_pattern_buffer(1, 1, 300, {0xAABB}).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(proto_expected_seq(), 2);

  EXPECT_EQ(port_test_fpga_emission_word(0, 0), 0x1234);
  EXPECT_EQ(port_test_fpga_emission_word(0, 1), 0x5678);
  EXPECT_EQ(port_test_fpga_emission_word(1, 300), 0xAABB);
  EXPECT_EQ(port_test_fpga_emission_word(0, 300), 0) << "banks must be independent";
}

TEST(Proto, WritePatternBufferCrossesPageBoundary) {
  reset_all();

  make_write_pattern_buffer(0, 0, FPGA_PAGE_WORDS - 2, {0x0001, 0x0002, 0x0003, 0x0004}).deliver();
  EXPECT_EQ(_sTx.data, 0);

  EXPECT_EQ(port_test_fpga_emission_word(0, FPGA_PAGE_WORDS - 2), 0x0001);
  EXPECT_EQ(port_test_fpga_emission_word(0, FPGA_PAGE_WORDS - 1), 0x0002);
  EXPECT_EQ(port_test_fpga_emission_word(0, FPGA_PAGE_WORDS), 0x0003);
  EXPECT_EQ(port_test_fpga_emission_word(0, FPGA_PAGE_WORDS + 1), 0x0004);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_MEM_WR_PAGE), 1) << "page register must have advanced";
}

TEST(Proto, WritePatternBufferRawSlotLayout) {
  reset_all();

  std::vector<uint16_t> pattern(NUM_TRANSDUCERS);
  for (uint16_t i = 0; i < NUM_TRANSDUCERS; ++i) {
    pattern[i] = static_cast<uint16_t>((i << 8) | (0xFF - (i & 0xFF)));
  }
  uint32_t slot = 3 * EMISSION_SLOT_WORDS;
  make_write_pattern_buffer(0, 0, slot, pattern).deliver();
  EXPECT_EQ(_sTx.data, 0);

  for (uint16_t i = 0; i < NUM_TRANSDUCERS; ++i) {
    ASSERT_EQ(port_test_fpga_emission_word(0, slot + i), pattern[i]) << "transducer " << i;
  }
}

TEST(Proto, WritePatternBufferEmptyDataIsNoOpSuccess) {
  reset_all();
  make_write_pattern_buffer(0, 0, 0, {}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, 0);
}

TEST(Proto, WritePatternBufferRejectsInvalidPayloads) {
  reset_all();

  make_write_pattern_buffer(0, NUM_BANKS, 0, {0x0001}).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  Frame f(1, CMD_WRITE_PATTERN_BUFFER);
  uint8_t* p = f.payload();
  p[EM_WRITE_OFFSET_BANK] = 0;
  put_u32_le(p + EM_WRITE_OFFSET_OFFSET, 0);
  put_u16_le(p + EM_WRITE_OFFSET_DATA_LEN, 3);
  f.deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  Frame g(2, CMD_WRITE_PATTERN_BUFFER);
  p = g.payload();
  p[EM_WRITE_OFFSET_BANK] = 0;
  put_u32_le(p + EM_WRITE_OFFSET_OFFSET, 0);
  put_u16_le(p + EM_WRITE_OFFSET_DATA_LEN, EM_WRITE_MAX_DATA_LEN + 2);
  g.deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_write_pattern_buffer(3, 0, EMISSION_RAM_WORDS - 1, {0x0001, 0x0002}).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  EXPECT_EQ(port_test_fpga_emission_word(0, EMISSION_RAM_WORDS - 1), 0);
}

TEST(Proto, WritePatternCompressedPhaseFullDecompressesTwoIndices) {
  reset_all();

  std::vector<uint16_t> words(NUM_TRANSDUCERS);
  for (uint16_t t = 0; t < NUM_TRANSDUCERS; ++t) {
    const uint8_t p0 = static_cast<uint8_t>(t & 0xFF);
    const uint8_t p1 = static_cast<uint8_t>(0xFF - (t & 0xFF));
    words[t] = static_cast<uint16_t>(p0 | (p1 << 8));
  }
  const uint32_t slot = 5 * EMISSION_SLOT_WORDS;
  make_write_pattern_compressed(0, 1, slot, WRITE_PATTERN_FORMAT_PHASE_FULL, 2, words).deliver();
  EXPECT_EQ(_sTx.data, 0);

  for (uint16_t t = 0; t < NUM_TRANSDUCERS; ++t) {
    const uint8_t p0 = static_cast<uint8_t>(t & 0xFF);
    const uint8_t p1 = static_cast<uint8_t>(0xFF - (t & 0xFF));
    ASSERT_EQ(port_test_fpga_emission_word(1, slot + t), static_cast<uint16_t>(0xFF00 | p0)) << "g0 t=" << t;
    ASSERT_EQ(port_test_fpga_emission_word(1, slot + EMISSION_SLOT_WORDS + t), static_cast<uint16_t>(0xFF00 | p1))
        << "g1 t=" << t;
  }
}

TEST(Proto, WritePatternCompressedPhaseFullPartialCountWritesSingleSlot) {
  reset_all();

  std::vector<uint16_t> words(NUM_TRANSDUCERS, 0x00AB);
  const uint32_t slot = 2 * EMISSION_SLOT_WORDS;
  make_write_pattern_compressed(0, 0, slot, WRITE_PATTERN_FORMAT_PHASE_FULL, 1, words).deliver();
  EXPECT_EQ(_sTx.data, 0);

  EXPECT_EQ(port_test_fpga_emission_word(0, slot), static_cast<uint16_t>(0xFF00 | 0xAB));
  EXPECT_EQ(port_test_fpga_emission_word(0, slot + EMISSION_SLOT_WORDS), 0) << "second slot untouched";
}

TEST(Proto, WritePatternCompressedPhaseHalfDecompressesFourIndices) {
  reset_all();

  std::vector<uint16_t> words(NUM_TRANSDUCERS);
  for (uint16_t t = 0; t < NUM_TRANSDUCERS; ++t) {
    const uint8_t n0 = static_cast<uint8_t>(t & 0x0F);
    const uint8_t n1 = static_cast<uint8_t>((t + 1) & 0x0F);
    const uint8_t n2 = static_cast<uint8_t>((t + 2) & 0x0F);
    const uint8_t n3 = static_cast<uint8_t>((t + 3) & 0x0F);
    words[t] = static_cast<uint16_t>(n0 | (n1 << 4) | (n2 << 8) | (n3 << 12));
  }
  const uint32_t slot = 7 * EMISSION_SLOT_WORDS;
  make_write_pattern_compressed(0, 0, slot, WRITE_PATTERN_FORMAT_PHASE_HALF, 4, words).deliver();
  EXPECT_EQ(_sTx.data, 0);

  for (uint16_t t = 0; t < NUM_TRANSDUCERS; ++t) {
    for (uint8_t g = 0; g < 4; ++g) {
      const uint8_t p4 = static_cast<uint8_t>((t + g) & 0x0F);
      const uint16_t expected = static_cast<uint16_t>(0xFF00 | (p4 << 4) | p4);
      ASSERT_EQ(port_test_fpga_emission_word(0, slot + g * EMISSION_SLOT_WORDS + t), expected) << "g=" << g << " t=" << t;
    }
  }
}

TEST(Proto, WritePatternCompressedRejectsInvalidPayloads) {
  reset_all();

  const std::vector<uint16_t> full(NUM_TRANSDUCERS, 0x1234);

  make_write_pattern_compressed(0, 0, 0, 0, 1, full).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_write_pattern_compressed(1, 0, 0, 3, 1, full).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_write_pattern_compressed(2, 0, 0, WRITE_PATTERN_FORMAT_PHASE_FULL, 0, full).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_write_pattern_compressed(3, 0, 0, WRITE_PATTERN_FORMAT_PHASE_FULL, 3, full).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_write_pattern_compressed(4, 0, 0, WRITE_PATTERN_FORMAT_PHASE_HALF, 5, full).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_write_pattern_compressed(5, 0, EMISSION_RAM_WORDS - EMISSION_SLOT_WORDS, WRITE_PATTERN_FORMAT_PHASE_FULL, 2, full)
      .deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
}


TEST(Proto, WriteModBufferPacksSamplesIntoWordsPerBank) {
  reset_all();

  make_write_mod_buffer(0, 0, 0, {0x10, 0x20, 0x30, 0x40}).deliver();
  EXPECT_EQ(_sTx.data, 0);
  make_write_mod_buffer(1, 1, 100, {0xAA, 0xBB}).deliver();
  EXPECT_EQ(_sTx.data, 0);

  EXPECT_EQ(port_test_fpga_mod_word(0, 0), 0x2010) << "LE sample pair packing";
  EXPECT_EQ(port_test_fpga_mod_word(0, 1), 0x4030);
  EXPECT_EQ(port_test_fpga_mod_word(1, 50), 0xBBAA) << "sample offset 100 = word 50";
  EXPECT_EQ(port_test_fpga_mod_word(0, 50), 0) << "banks must be independent";
}

TEST(Proto, WriteModBufferOddLengthPadsHighByte) {
  reset_all();
  make_write_mod_buffer(0, 0, 0, {0xAA}).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_mod_word(0, 0), 0x00AA);
}

TEST(Proto, WriteModBufferCrossesPageBoundary) {
  reset_all();

  
  uint32_t offset = 2 * FPGA_PAGE_WORDS - 2;
  make_write_mod_buffer(0, 0, offset, {0x01, 0x02, 0x03, 0x04}).deliver();
  EXPECT_EQ(_sTx.data, 0);

  EXPECT_EQ(port_test_fpga_mod_word(0, FPGA_PAGE_WORDS - 1), 0x0201);
  EXPECT_EQ(port_test_fpga_mod_word(0, FPGA_PAGE_WORDS), 0x0403);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_MEM_WR_PAGE), 1) << "page register must have advanced";
}

TEST(Proto, WriteModBufferAcceptsChunkedWritesUpToCapacity) {
  reset_all();

  
  
  uint8_t seq = 0;
  uint32_t written = 0;
  while (written < MOD_BUFFER_SAMPLES) {
    uint16_t len = static_cast<uint16_t>(std::min<uint32_t>(MOD_WRITE_MAX_DATA_LEN, MOD_BUFFER_SAMPLES - written));
    std::vector<uint8_t> chunk(len, static_cast<uint8_t>(written >> 8));
    make_write_mod_buffer(seq++, 0, written, chunk).deliver();
    ASSERT_EQ(_sTx.data, 0) << "chunk at offset " << written;
    written += len;
  }
  uint16_t expected = static_cast<uint16_t>((MOD_BUFFER_SAMPLES - 1) >> 8);
  expected = static_cast<uint16_t>(expected | (expected << 8));
  EXPECT_EQ(port_test_fpga_mod_word(0, MOD_BUFFER_SAMPLES / 2 - 1), expected);
}

TEST(Proto, WriteModBufferEmptyDataIsNoOpSuccess) {
  reset_all();
  make_write_mod_buffer(0, 0, 0, {}).deliver();
  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, 0);
}

TEST(Proto, WriteModBufferRejectsInvalidPayloads) {
  reset_all();

  
  make_write_mod_buffer(0, NUM_BANKS, 0, {0x01}).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  Frame(1, CMD_READ_ERROR_DETAIL).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  
  make_write_mod_buffer(2, 0, 1, {0x01, 0x02}).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  
  Frame f(3, CMD_WRITE_MOD_BUFFER);
  uint8_t* p = f.payload();
  p[MOD_WRITE_OFFSET_BANK] = 0;
  put_u32_le(p + MOD_WRITE_OFFSET_OFFSET, 0);
  put_u16_le(p + MOD_WRITE_OFFSET_DATA_LEN, MOD_WRITE_MAX_DATA_LEN + 1);
  f.deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  
  make_write_mod_buffer(4, 0, MOD_BUFFER_SAMPLES - 2, {0x01, 0x02, 0x03}).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  EXPECT_EQ(port_test_fpga_mod_word(0, MOD_BUFFER_SAMPLES / 2 - 1), 0);
}



TEST(Proto, ConfigModWritesPlaybackRegistersAndLatches) {
  reset_all();
  const uint32_t latches_at_boot = port_test_fpga_latch_count(CTL_FLAG_MOD_SET);

  make_config_mod(0, 1, 10, 4000).deliver();

  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_CYCLE0 + 1), 3999) << "CYCLE = size - 1";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0 + 1), 10);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REP0 + 1), REP_INFINITE);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_TRANSITION_MODE), TRANSITION_MODE_SYNC_IDX)
      << "config must not touch transition mode (kept at boot default)";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REQ_RD_BANK), 0) << "config must not switch the playback bank";
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_MOD_SET), latches_at_boot + 1);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_MOD_SET, 0) << "the FPGA clears the latch bit";
}

TEST(Proto, ConfigModWritesFiniteLoopRep) {
  reset_all();

  make_config_mod(0, 0, 10, 4000, 9).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REP0), 9) << "REP = loop_count - 1";
}

TEST(Proto, ConfigModRejectsInvalidFieldsAndLeavesRegistersUntouched) {
  reset_all();
  make_config_mod(0, 1, 2, 100).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_config_mod(1, NUM_BANKS, 1, 1).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  make_config_mod(2, 0, 0, 1).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  make_config_mod(3, 0, 1, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  make_config_mod(4, 0, 1, MOD_BUFFER_SAMPLES + 1).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_CYCLE0 + 1), 99);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0 + 1), 2);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REQ_RD_BANK), 0) << "config never writes the playback bank";
}

TEST(Proto, ConfigModAcceptsFullBufferSize) {
  reset_all();
  make_config_mod(0, 0, 1, MOD_BUFFER_SAMPLES).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_CYCLE0), 0xFFFF) << "65536 - 1";
}



TEST(Proto, ConfigPatternRawWritesRegistersAndLatches) {
  reset_all();

  make_config_pattern(0, 0, EMISSION_TYPE_RAW, 2, EMISSION_MAX_INDICES, 0, 0).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_MODE0), EMISSION_TYPE_RAW);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_CYCLE0), EMISSION_MAX_INDICES - 1);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_FREQ_DIV0), 2);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REP0), REP_INFINITE);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_TRANSITION_MODE), TRANSITION_MODE_SYNC_IDX)
      << "config must not touch transition mode (kept at boot default)";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_PATTERN_SET, 0);
}

TEST(Proto, ConfigPatternFociWritesRegistersAndLatches) {
  reset_all();

  make_config_pattern(0, 1, EMISSION_TYPE_FOCI, 1, 8192, 8, 340).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_MODE0 + 1), EMISSION_TYPE_FOCI);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_CYCLE0 + 1), 8191);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_SOUND_SPEED0 + 1), 340);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_NUM_FOCI0 + 1), 8);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REP0 + 1), REP_INFINITE);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REQ_RD_BANK), 0) << "config must not switch the playback bank";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_PATTERN_SET, 0);
}

TEST(Proto, ConfigPatternWritesFiniteLoopRep) {
  reset_all();

  make_config_pattern(0, 0, EMISSION_TYPE_RAW, 2, EMISSION_MAX_INDICES, 0, 0, 4).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REP0), 4) << "REP = loop_count - 1";
}

TEST(Proto, ConfigPatternRejectsInvalidRawFields) {
  reset_all();

  make_config_pattern(0, 0, EMISSION_TYPE_RAW, 1, EMISSION_MAX_INDICES + 1, 0, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_config_pattern(1, 0, 2, 1, 1, 0, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_CYCLE0), 0) << "registers must stay untouched";
}

TEST(Proto, ConfigPatternRejectsInvalidFociFields) {
  reset_all();

  
  make_config_pattern(0, 0, EMISSION_TYPE_FOCI, 1, 1, 0, 340).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  make_config_pattern(1, 0, EMISSION_TYPE_FOCI, 1, 1, NUM_FOCI_MAX + 1, 340).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_config_pattern(2, 0, EMISSION_TYPE_FOCI, 1, MAX_FOCI_TOTAL / 8 + 1, 8, 340).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_config_pattern(3, 0, EMISSION_TYPE_FOCI, 1, 1, 1, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  make_config_pattern(4, 0, EMISSION_TYPE_FOCI, 1, MAX_FOCI_TOTAL / 8, 8, 340).deliver();
  EXPECT_EQ(_sTx.data, 0);
}



TEST(Proto, ChangePatternBankWritesTransitionAndReqBankAndLatches) {
  reset_all();
  const uint32_t latches_at_boot = port_test_fpga_latch_count(CTL_FLAG_PATTERN_SET);

  make_change_pattern_bank(0, 1, TRANSITION_MODE_IMMEDIATE, 0).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_TRANSITION_MODE), TRANSITION_MODE_IMMEDIATE);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_PATTERN_SET), latches_at_boot + 1);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_PATTERN_SET, 0) << "the FPGA clears the latch bit";
}

TEST(Proto, ChangePatternBankWritesTransitionValue) {
  reset_all();

  make_change_pattern_bank(0, 0, TRANSITION_MODE_SYS_TIME, 0x0123456789ABCDEFull).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_TRANSITION_MODE), TRANSITION_MODE_SYS_TIME);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_TRANSITION_VALUE_0), 0xCDEF);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 1), 0x89AB);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 2), 0x4567);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 3), 0x0123);
}

TEST(Proto, ChangePatternBankRejectsInvalidBank) {
  reset_all();
  make_change_pattern_bank(0, NUM_BANKS, TRANSITION_MODE_IMMEDIATE, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REQ_RD_BANK), 0) << "rejected change must not switch the bank";
}

TEST(Proto, ChangeModBankWritesTransitionAndReqBankAndLatches) {
  reset_all();
  const uint32_t latches_at_boot = port_test_fpga_latch_count(CTL_FLAG_MOD_SET);

  make_change_mod_bank(0, 1, TRANSITION_MODE_IMMEDIATE, 0).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_TRANSITION_MODE), TRANSITION_MODE_IMMEDIATE);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REQ_RD_BANK), 1);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_MOD_SET), latches_at_boot + 1);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_MOD_SET, 0) << "the FPGA clears the latch bit";
}

TEST(Proto, ChangeModBankRejectsInvalidBank) {
  reset_all();
  make_change_mod_bank(0, NUM_BANKS, TRANSITION_MODE_IMMEDIATE, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REQ_RD_BANK), 0) << "rejected change must not switch the bank";
}



TEST(Proto, SetSilencerFixedCompletionStepsWritesRegistersAndLatches) {
  reset_all();
  const uint32_t latches_at_boot = port_test_fpga_latch_count(CTL_FLAG_SILENCER_SET);

  make_set_silencer(0, SILENCER_FLAG_STRICT_MODE, 256, 256, 5, 7).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_FLAG), SILENCER_FLAG_STRICT_MODE) << "strict mode flag passed through";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 5);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 7);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_SILENCER_SET), latches_at_boot + 1);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_SILENCER_SET, 0) << "the FPGA clears the latch bit";
}

TEST(Proto, SetSilencerFixedUpdateRateWritesRegistersAndLatches) {
  reset_all();
  const uint32_t latches_at_boot = port_test_fpga_latch_count(CTL_FLAG_SILENCER_SET);

  make_set_silencer(0, SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, 8, 16, 10, 40).deliver();

  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_FLAG), SILENCER_FLAG_FIXED_UPDATE_RATE_MODE);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 8);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 16);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_SILENCER_SET), latches_at_boot + 1);
}

TEST(Proto, SetSilencerRejectsZeroCompletionStepsInStepsMode) {
  reset_all();

  make_set_silencer(0, 0, 256, 256, 0, 7).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  make_set_silencer(1, 0, 256, 256, 5, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10) << "registers must stay at boot default";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
}

TEST(Proto, SetSilencerRejectsZeroUpdateRateInRateMode) {
  reset_all();

  make_set_silencer(0, SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, 0, 16, 10, 40).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
  make_set_silencer(1, SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, 8, 0, 10, 40).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);

  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256) << "registers must stay at boot default";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_FLAG), 0) << "rejected set must not flip the mode flag";
}

TEST(Proto, SetSilencerStepsModeIgnoresZeroUpdateRate) {
  reset_all();

  make_set_silencer(0, 0, 0, 0, 5, 7).deliver();
  EXPECT_EQ(_sTx.data, 0) << "completion-steps mode does not validate update_rate";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 5);
}

TEST(Proto, StrictSilencerRejectsTooFastModConfig) {
  reset_all();
  make_set_silencer(0, SILENCER_FLAG_STRICT_MODE, 256, 256, 10, 40).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_config_mod(1, 0, 9, 100).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_SILENCER_SETTING) << "mod sampling faster than intensity completion is rejected";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0), 0xFFFF) << "rejected config must not write the divider";

  make_config_mod(2, 0, 10, 100).deliver();
  EXPECT_EQ(_sTx.data, 0) << "sampling period equal to completion is allowed";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0), 10);
}

TEST(Proto, StrictSilencerRejectsTooFastPatternConfig) {
  reset_all();
  make_set_silencer(0, SILENCER_FLAG_STRICT_MODE, 256, 256, 10, 40).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_config_pattern(1, 0, EMISSION_TYPE_RAW, 20, 1, 0, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_SILENCER_SETTING) << "pattern drives phase: divider below phase completion rejected";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_FREQ_DIV0), 0xFFFF);

  make_config_pattern(2, 0, EMISSION_TYPE_RAW, 40, 1, 0, 0).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_FREQ_DIV0), 40);
}

TEST(Proto, NonStrictSilencerDoesNotGuardSampling) {
  reset_all();
  make_set_silencer(0, 0, 256, 256, 10, 40).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_config_mod(1, 0, 1, 100).deliver();
  EXPECT_EQ(_sTx.data, 0) << "non-strict silencer does not guard the sampling rate";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0), 1);
}

TEST(Proto, StrictSilencerRejectedWhenActiveSamplingTooFast) {
  reset_all();
  make_config_mod(0, 0, 5, 100).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_set_silencer(1, SILENCER_FLAG_STRICT_MODE, 256, 256, 8, 40).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_SILENCER_SETTING) << "completion longer than active mod sampling is rejected";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10) << "boot default must stay unchanged";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_FLAG), 0) << "rejected silencer must not change the flag";
}

TEST(Proto, FixedUpdateRateModeReleasesGuard) {
  reset_all();
  make_set_silencer(0, SILENCER_FLAG_STRICT_MODE, 256, 256, 10, 40).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_set_silencer(1, SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, 8, 16, 10, 40).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_config_mod(2, 0, 1, 100).deliver();
  EXPECT_EQ(_sTx.data, 0) << "fixed-update-rate mode releases the strict guard";
}

TEST(Proto, StrictSilencerRejectsSwitchToTooFastBank) {
  reset_all();
  make_config_mod(0, 1, 5, 100).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_set_silencer(1, SILENCER_FLAG_STRICT_MODE, 256, 256, 10, 40).deliver();
  ASSERT_EQ(_sTx.data, 0) << "active bank 0 sampling is still default, so strict silencer is accepted";

  make_change_mod_bank(2, 1, TRANSITION_MODE_IMMEDIATE, 0).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_SILENCER_SETTING) << "switching to a bank whose sampling is too fast is rejected";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REQ_RD_BANK), 0) << "rejected switch must not change the bank";
}



TEST(Proto, ClearReleasesStrictSilencerGuard) {
  reset_all();
  make_set_silencer(0, SILENCER_FLAG_STRICT_MODE, 256, 256, 10, 40).deliver();
  ASSERT_EQ(_sTx.data, 0);

  make_config_mod(1, 0, 5, 100).deliver();
  ASSERT_EQ(_sTx.data, ERR_INVALID_SILENCER_SETTING) << "strict guard rejects too-fast sampling";

  Frame(2, CMD_CLEAR).deliver();
  EXPECT_EQ(_sTx.data, 0) << "Clear is accepted";

  make_config_mod(3, 0, 5, 100).deliver();
  EXPECT_EQ(_sTx.data, 0) << "Clear releases the strict guard";
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0), 5);
}

TEST(Proto, ClearRestoresSilencerAndBankBaseline) {
  reset_all();
  make_set_silencer(0, SILENCER_FLAG_STRICT_MODE, 256, 256, 20, 30).deliver();
  ASSERT_EQ(_sTx.data, 0);
  make_config_mod(1, 1, 50, 100).deliver();
  ASSERT_EQ(_sTx.data, 0);
  make_change_mod_bank(2, 1, TRANSITION_MODE_IMMEDIATE, 0).deliver();
  ASSERT_EQ(_sTx.data, 0) << "bank 1 sampling (50) satisfies completion 20, so the switch is accepted";

  Frame(3, CMD_CLEAR).deliver();
  ASSERT_EQ(_sTx.data, 0);

  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_FLAG), 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
  for (uint8_t bank = 0; bank < NUM_BANKS; ++bank) {
    EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0 + bank), 0xFFFF);
    EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_FREQ_DIV0 + bank), 0xFFFF);
  }
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REQ_RD_BANK), 0) << "active modulation bank back to 0";
}

TEST(Proto, BootBringsFpgaToLegacyClearBaseline) {
  reset_all();

  
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_FLAG), 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);

  
  for (uint8_t bank = 0; bank < NUM_BANKS; ++bank) {
    EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_CYCLE0 + bank), 1);
    EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_FREQ_DIV0 + bank), 0xFFFF);
    EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_REP0 + bank), REP_INFINITE);
    EXPECT_EQ(port_test_fpga_mod_word(bank, 0), 0xFFFF);
  }

  
  for (uint8_t bank = 0; bank < NUM_BANKS; ++bank) {
    EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_MODE0 + bank), EMISSION_TYPE_RAW);
    EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_CYCLE0 + bank), 0);
    EXPECT_EQ(port_test_fpga_ctl(ADDR_PATTERN_REP0 + bank), REP_INFINITE);
    EXPECT_EQ(port_test_fpga_emission_word(bank, 0), 0);
    EXPECT_EQ(port_test_fpga_emission_word(bank, NUM_TRANSDUCERS - 1), 0);
  }

  
  EXPECT_EQ(port_test_fpga_phase_corr(0), 0);
  EXPECT_EQ(port_test_fpga_phase_corr(PHASE_CORR_WORDS - 1), 0);
  EXPECT_EQ(port_test_fpga_output_mask(0), 0xFFFF);
  EXPECT_EQ(port_test_fpga_output_mask(OUTPUT_MASK_WORDS - 1), 0xFFFF);

  
  EXPECT_EQ(port_test_fpga_pwe_word(0), 0x00);
  EXPECT_EQ(port_test_fpga_pwe_word(1), 0x01);
  EXPECT_EQ(port_test_fpga_pwe_word(128), 0x56);
  EXPECT_EQ(port_test_fpga_pwe_word(PWE_TABLE_SIZE - 1), 0x100);

  
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_MOD_SET), 1u);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_PATTERN_SET), 1u);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_SILENCER_SET), 1u);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_DEBUG_SET), 1u);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_SYNC_SET), 0u);
}



TEST(Proto, SynchronizeWritesNextSync0AndLatches) {
  reset_all();
  port_test_set_next_sync0(0x1122334455667788ULL);

  Frame(0, CMD_SYNCHRONIZE).deliver();

  EXPECT_EQ(_sTx.ack, 0);
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_ECAT_SYNC_TIME_0), 0x7788);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_ECAT_SYNC_TIME_0 + 1), 0x5566);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_ECAT_SYNC_TIME_0 + 2), 0x3344);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_ECAT_SYNC_TIME_0 + 3), 0x1122);
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_SYNC_SET), 1u);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_SYNC_SET, 0);
}



TEST(Proto, FpgaStateSurvivesReset) {
  reset_all();

  make_write_pattern_buffer(0, 0, 0, {0x5A5A}).deliver();
  make_write_mod_buffer(1, 1, 8, {0x77}).deliver();
  make_config_mod(2, 1, 5, 256).deliver();
  ASSERT_EQ(_sTx.data, 0);

  Frame(99 , CMD_RESET).deliver();
  ASSERT_EQ(proto_expected_seq(), 0);

  
  EXPECT_EQ(port_test_fpga_emission_word(0, 0), 0x5A5A);
  EXPECT_EQ(port_test_fpga_mod_word(1, 4), 0x0077);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_MOD_CYCLE0 + 1), 255);
}



TEST(Proto, StructSizesMatchSpec) {
  EXPECT_EQ(sizeof(rx_frame_t), 626u);
  
  
  
  EXPECT_EQ(sizeof(tx_frame_t), 4u);
  EXPECT_EQ(WIRE_RX_FRAME_BYTES, 628u);
}

TEST(Proto, ForceFanSetsAndClearsPersistentBit) {
  reset_all();

  make_force_fan(0, 1).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN, CTL_FLAG_FORCE_FAN);

  make_force_fan(1, 0).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN, 0);
}

TEST(Proto, ForceFanRejectsOutOfRange) {
  reset_all();
  make_force_fan(0, 2).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
}

TEST(Proto, ForceFanSurvivesSubsequentLatch) {
  reset_all();
  make_force_fan(0, 1).deliver();
  make_set_silencer(1, SILENCER_FLAG_STRICT_MODE, 256, 256, 5, 7).deliver();
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN, CTL_FLAG_FORCE_FAN);
}

TEST(Proto, EmulateGpioInMapsBits) {
  reset_all();
  make_gpio_in(0, 0b1010).deliver();
  EXPECT_EQ(_sTx.data, 0);
  const uint16_t ctl = port_test_fpga_ctl(ADDR_CTL_FLAG);
  EXPECT_EQ(ctl & CTL_FLAG_GPIO_IN_0, 0);
  EXPECT_EQ(ctl & CTL_FLAG_GPIO_IN_1, CTL_FLAG_GPIO_IN_1);
  EXPECT_EQ(ctl & CTL_FLAG_GPIO_IN_2, 0);
  EXPECT_EQ(ctl & CTL_FLAG_GPIO_IN_3, CTL_FLAG_GPIO_IN_3);
}

TEST(Proto, EmulateGpioInRejectsOutOfRange) {
  reset_all();
  make_gpio_in(0, 0x10).deliver();
  EXPECT_EQ(_sTx.data, ERR_INVALID_PAYLOAD);
}

TEST(Proto, PhaseCorrPacksBytesIntoWords) {
  reset_all();
  std::vector<uint8_t> phases(NUM_TRANSDUCERS);
  for (uint16_t i = 0; i < NUM_TRANSDUCERS; ++i) phases[i] = static_cast<uint8_t>(i & 0xFF);
  make_phase_corr(0, phases).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_phase_corr(0), static_cast<uint16_t>(phases[0] | (phases[1] << 8)));
  EXPECT_EQ(port_test_fpga_phase_corr(1), static_cast<uint16_t>(phases[2] | (phases[3] << 8)));
  // Last (odd) transducer high byte is zero-padded.
  EXPECT_EQ(port_test_fpga_phase_corr(124), static_cast<uint16_t>(phases[248]));
}

TEST(Proto, OutputMaskWritesWords) {
  reset_all();
  std::vector<uint16_t> words(OUTPUT_MASK_USED_WORDS);
  for (size_t i = 0; i < words.size(); ++i) words[i] = static_cast<uint16_t>(0x1000 + i);
  make_output_mask(0, words).deliver();
  EXPECT_EQ(_sTx.data, 0);
  for (size_t i = 0; i < words.size(); ++i) {
    EXPECT_EQ(port_test_fpga_output_mask(static_cast<uint16_t>(i)), words[i]);
  }
}

TEST(Proto, PweWritesTable) {
  reset_all();
  std::vector<uint16_t> table(PWE_TABLE_SIZE);
  for (size_t i = 0; i < table.size(); ++i) table[i] = static_cast<uint16_t>(i);
  make_pwe(0, table).deliver();
  EXPECT_EQ(_sTx.data, 0);
  EXPECT_EQ(port_test_fpga_pwe_word(0), 0);
  EXPECT_EQ(port_test_fpga_pwe_word(1), 1);
  EXPECT_EQ(port_test_fpga_pwe_word(255), 255);
}

TEST(Proto, GpioOutWritesDebugValuesAndLatches) {
  reset_all();
  const uint32_t latches_at_boot = port_test_fpga_latch_count(CTL_FLAG_DEBUG_SET);
  const std::vector<uint64_t> values = {0x0102030405060708ull, 0x1112131415161718ull,
                                        0x2122232425262728ull, 0x3132333435363738ull};
  make_gpio_out(0, values).deliver();
  EXPECT_EQ(_sTx.data, 0);
  for (int v = 0; v < 4; ++v) {
    for (int w = 0; w < 4; ++w) {
      const uint16_t expect = static_cast<uint16_t>((values[v] >> (16 * w)) & 0xFFFF);
      EXPECT_EQ(port_test_fpga_ctl(ADDR_DEBUG_VALUE0_0 + v * 4 + w), expect);
    }
  }
  EXPECT_EQ(port_test_fpga_latch_count(CTL_FLAG_DEBUG_SET), latches_at_boot + 1);
}

TEST(Proto, ReadFpgaStateReturnsRegisterByte) {
  reset_all();
  port_test_fpga_set_controller(ADDR_FPGA_STATE, 0x83);
  Frame(0, CMD_READ_FPGA_STATE).deliver();
  EXPECT_EQ(_sTx.data, 0x83);
}

TEST(Proto, ClearResetsForceFan) {
  reset_all();
  make_force_fan(0, 1).deliver();
  Frame(1, CMD_CLEAR).deliver();
  EXPECT_EQ(port_test_fpga_ctl(ADDR_CTL_FLAG) & CTL_FLAG_FORCE_FAN, 0);
}
