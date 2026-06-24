#ifdef __cplusplus
extern "C" {
#endif

#include "fpga.h"

#include <stdint.h>

#include "app.h"
#include "proto.h"

void fpga_write(uint8_t select, uint16_t addr, uint16_t value) {
  port_fpga_write((uint16_t)(((uint16_t)select << 14) | (addr & 0x3FFFu)), value);
}

uint16_t fpga_read(uint8_t select, uint16_t addr) {
  return port_fpga_read((uint16_t)(((uint16_t)select << 14) | (addr & 0x3FFFu)));
}

void set_and_wait_update(uint16_t flag) {
  uint16_t persistent = fpga_read(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, (uint16_t)(persistent | flag));
  while ((fpga_read(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG) & flag) != 0u) {
  }
}

void fpga_write_u64(uint16_t addr, uint64_t value) {
  for (uint16_t i = 0; i < 4u; i++) {
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(addr + i), (uint16_t)(value >> (16u * i)));
  }
}

void fpga_write_change_bank(uint16_t req_rd_bank_addr, uint16_t transition_mode_addr, uint16_t transition_value_addr,
                            uint8_t bank, uint8_t transition_mode, uint64_t transition_value) {
  fpga_write(BRAM_SELECT_CONTROLLER, transition_mode_addr, transition_mode);
  fpga_write_u64(transition_value_addr, transition_value);
  fpga_write(BRAM_SELECT_CONTROLLER, req_rd_bank_addr, bank);
}

void fpga_write_ram(uint8_t select, uint16_t wr_bank_reg, uint16_t wr_page_reg, uint8_t bank, uint32_t offset,
                    const uint8_t* src, uint16_t len_bytes) {
  fpga_write(BRAM_SELECT_CONTROLLER, wr_bank_reg, bank);
  uint32_t page = offset / FPGA_PAGE_WORDS;
  fpga_write(BRAM_SELECT_CONTROLLER, wr_page_reg, (uint16_t)page);
  uint16_t n_words = (uint16_t)(((uint32_t)len_bytes + 1u) / 2u);
  for (uint16_t i = 0; i < n_words; i++) {
    uint32_t word_idx = offset + i;
    uint32_t p = word_idx / FPGA_PAGE_WORDS;
    if (p != page) {
      page = p;
      fpga_write(BRAM_SELECT_CONTROLLER, wr_page_reg, (uint16_t)page);
    }
    uint16_t lo = src[2u * i];
    uint16_t hi = ((uint32_t)(2u * i + 1u) < len_bytes) ? src[2u * i + 1u] : 0u;
    fpga_write(select, (uint16_t)(word_idx % FPGA_PAGE_WORDS), (uint16_t)(lo | (hi << 8)));
  }
}

static const uint8_t ASIN_TABLE[PWE_TABLE_SIZE] = {
    0x00, 0x01, 0x01, 0x02, 0x03, 0x03, 0x04, 0x04, 0x05, 0x06, 0x06, 0x07, 0x08, 0x08, 0x09, 0x0a, 0x0a, 0x0b, 0x0c,
    0x0c, 0x0d, 0x0d, 0x0e, 0x0f, 0x0f, 0x10, 0x11, 0x11, 0x12, 0x13, 0x13, 0x14, 0x15, 0x15, 0x16, 0x16, 0x17, 0x18,
    0x18, 0x19, 0x1a, 0x1a, 0x1b, 0x1c, 0x1c, 0x1d, 0x1e, 0x1e, 0x1f, 0x20, 0x20, 0x21, 0x21, 0x22, 0x23, 0x23, 0x24,
    0x25, 0x25, 0x26, 0x27, 0x27, 0x28, 0x29, 0x29, 0x2a, 0x2b, 0x2b, 0x2c, 0x2d, 0x2d, 0x2e, 0x2f, 0x2f, 0x30, 0x31,
    0x31, 0x32, 0x33, 0x33, 0x34, 0x35, 0x35, 0x36, 0x37, 0x37, 0x38, 0x39, 0x39, 0x3a, 0x3b, 0x3b, 0x3c, 0x3d, 0x3e,
    0x3e, 0x3f, 0x40, 0x40, 0x41, 0x42, 0x42, 0x43, 0x44, 0x44, 0x45, 0x46, 0x47, 0x47, 0x48, 0x49, 0x49, 0x4a, 0x4b,
    0x4c, 0x4c, 0x4d, 0x4e, 0x4e, 0x4f, 0x50, 0x51, 0x51, 0x52, 0x53, 0x53, 0x54, 0x55, 0x56, 0x56, 0x57, 0x58, 0x59,
    0x59, 0x5a, 0x5b, 0x5c, 0x5c, 0x5d, 0x5e, 0x5f, 0x5f, 0x60, 0x61, 0x62, 0x63, 0x63, 0x64, 0x65, 0x66, 0x66, 0x67,
    0x68, 0x69, 0x6a, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x74, 0x75, 0x76, 0x77,
    0x78, 0x79, 0x7a, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x7f, 0x80, 0x81, 0x82, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88,
    0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b,
    0x9d, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2, 0xa3, 0xa5, 0xa6, 0xa7, 0xa8, 0xaa, 0xab, 0xac, 0xad, 0xaf, 0xb0, 0xb2, 0xb3,
    0xb4, 0xb6, 0xb7, 0xb9, 0xba, 0xbc, 0xbd, 0xbf, 0xc1, 0xc2, 0xc4, 0xc6, 0xc8, 0xca, 0xcc, 0xce, 0xd0, 0xd2, 0xd5,
    0xd7, 0xda, 0xdd, 0xe0, 0xe3, 0xe7, 0xec, 0xf2, 0x00};

void fpga_init(void) {
  uint16_t i;
  uint8_t bank;

  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, 0u);

  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_UPDATE_RATE_INTENSITY, SILENCER_DEFAULT_UPDATE_RATE);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_UPDATE_RATE_PHASE, SILENCER_DEFAULT_UPDATE_RATE);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_FLAG, 0u);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
             SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_SILENCER_COMPLETION_STEPS_PHASE, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE);

  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_MOD_TRANSITION_MODE, TRANSITION_MODE_SYNC_IDX);
  fpga_write_u64(ADDR_MOD_TRANSITION_VALUE_0, 0u);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_MOD_REQ_RD_BANK, 0u);
  for (bank = 0; bank < NUM_BANKS; bank++) {
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_CYCLE0 + bank), 1u);
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_FREQ_DIV0 + bank), 0xFFFFu);
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_MOD_REP0 + bank), REP_INFINITE);
    fpga_write(BRAM_SELECT_CONTROLLER, ADDR_MOD_MEM_WR_BANK, bank);
    fpga_write(BRAM_SELECT_CONTROLLER, ADDR_MOD_MEM_WR_PAGE, 0u);
    fpga_write(BRAM_SELECT_MOD, 0u, 0xFFFFu);
  }

  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_PATTERN_TRANSITION_MODE, TRANSITION_MODE_SYNC_IDX);
  fpga_write_u64(ADDR_PATTERN_TRANSITION_VALUE_0, 0u);
  fpga_write(BRAM_SELECT_CONTROLLER, ADDR_PATTERN_REQ_RD_BANK, 0u);
  for (bank = 0; bank < NUM_BANKS; bank++) {
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_MODE0 + bank), EMISSION_TYPE_RAW);
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_CYCLE0 + bank), 0u);
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_FREQ_DIV0 + bank), 0xFFFFu);
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_PATTERN_REP0 + bank), REP_INFINITE);
    fpga_write(BRAM_SELECT_CONTROLLER, ADDR_PATTERN_MEM_WR_BANK, bank);
    fpga_write(BRAM_SELECT_CONTROLLER, ADDR_PATTERN_MEM_WR_PAGE, 0u);
    for (i = 0; i < NUM_TRANSDUCERS; i++) {
      fpga_write(BRAM_SELECT_EMISSION, i, 0u);
    }
  }

  for (i = 0; i < PHASE_CORR_WORDS; i++) {
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(((uint16_t)BRAM_CNT_SELECT_PHASE_CORR << 8) | i), 0u);
  }
  for (i = 0; i < OUTPUT_MASK_WORDS; i++) {
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(((uint16_t)BRAM_CNT_SELECT_OUTPUT_MASK << 8) | i), 0xFFFFu);
  }

  for (i = 0; i < PWE_TABLE_SIZE; i++) {
    fpga_write(BRAM_SELECT_PWE_TABLE, i, ASIN_TABLE[i]);
  }
  fpga_write(BRAM_SELECT_PWE_TABLE, PWE_TABLE_SIZE - 1u, 0x100u);

  for (i = 0; i < DEBUG_VALUE_WORDS; i++) {
    fpga_write(BRAM_SELECT_CONTROLLER, (uint16_t)(ADDR_DEBUG_VALUE0_0 + i), 0u);
  }

  set_and_wait_update(CTL_FLAG_MOD_SET);
  set_and_wait_update(CTL_FLAG_PATTERN_SET);
  set_and_wait_update(CTL_FLAG_SILENCER_SET);
  set_and_wait_update(CTL_FLAG_DEBUG_SET);
}

#ifdef __cplusplus
}
#endif
