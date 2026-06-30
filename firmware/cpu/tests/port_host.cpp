#include <cstdint>
#include <cstring>

extern "C" {
#include "fpga.h"
#include "proto.h"
}

tx_frame_t _sTx = tx_frame_t{};

static uint32_t g_total_sleep_ms = 0;

extern "C" void port_sleep_ms(uint16_t ms) { g_total_sleep_ms += ms; }

extern "C" uint32_t port_test_total_sleep_ms() { return g_total_sleep_ms; }

extern "C" void port_test_reset_sleep() { g_total_sleep_ms = 0; }

namespace {
constexpr uint32_t kModWords = MOD_BUFFER_SAMPLES / 2;
constexpr uint16_t kLatchMask =
    CTL_FLAG_MOD_SET | CTL_FLAG_PATTERN_SET | CTL_FLAG_SILENCER_SET | CTL_FLAG_DEBUG_SET | CTL_FLAG_SYNC_SET;

uint16_t g_ctl[256];
uint16_t g_phase_corr[256];
uint16_t g_output_mask[OUTPUT_MASK_WORDS];
uint16_t g_pwe[PWE_TABLE_SIZE];
uint16_t g_mod_ram[NUM_BANKS][kModWords];
uint16_t g_em_ram[NUM_BANKS][EMISSION_RAM_WORDS];
uint32_t g_latch_count[16];
uint64_t g_next_sync0 = 0;
uint64_t g_dc_sys_time = 0;

void write_controller(uint32_t a, uint16_t value) {
  switch (a >> 8) {
    case BRAM_CNT_SELECT_MAIN:
      if (a == ADDR_CTL_FLAG) {
        for (int bit = 0; bit < 16; ++bit) {
          if ((value & kLatchMask & (1u << bit)) != 0) ++g_latch_count[bit];
        }
        g_ctl[ADDR_CTL_FLAG] = value & ~kLatchMask;
      } else {
        g_ctl[a & 0xFF] = value;
      }
      break;
    case BRAM_CNT_SELECT_PHASE_CORR:
      g_phase_corr[a & 0xFF] = value;
      break;
    case BRAM_CNT_SELECT_OUTPUT_MASK:
      g_output_mask[a & (OUTPUT_MASK_WORDS - 1)] = value;
      break;
    default:
      break;
  }
}
}  // namespace

extern "C" void port_fpga_write(uint16_t addr, uint16_t value) {
  const uint8_t select = (addr >> 14) & 0x3;
  const uint32_t a = addr & 0x3FFF;
  switch (select) {
    case BRAM_SELECT_CONTROLLER:
      write_controller(a, value);
      break;
    case BRAM_SELECT_MOD: {
      const uint16_t bank = g_ctl[ADDR_MOD_MEM_WR_BANK];
      const uint32_t page = g_ctl[ADDR_MOD_MEM_WR_PAGE];
      g_mod_ram[bank][(page << 14) | a] = value;
      break;
    }
    case BRAM_SELECT_PWE_TABLE:
      g_pwe[a & (PWE_TABLE_SIZE - 1)] = value;
      break;
    case BRAM_SELECT_EMISSION: {
      const uint16_t bank = g_ctl[ADDR_PATTERN_MEM_WR_BANK];
      const uint32_t page = g_ctl[ADDR_PATTERN_MEM_WR_PAGE];
      g_em_ram[bank][(page << 14) | a] = value;
      break;
    }
    default:
      break;
  }
}

extern "C" uint16_t port_fpga_read(uint16_t addr) {
  const uint8_t select = (addr >> 14) & 0x3;
  const uint32_t a = addr & 0x3FFF;
  if (select == BRAM_SELECT_CONTROLLER && (a >> 8) == BRAM_CNT_SELECT_MAIN) {
    return g_ctl[a & 0xFF];
  }
  return 0;
}

extern "C" uint64_t port_next_sync0() { return g_next_sync0; }

extern "C" uint64_t port_dc_sys_time() { return g_dc_sys_time; }

extern "C" void port_test_fpga_set_controller(uint16_t addr, uint16_t value) { g_ctl[addr & 0xFF] = value; }

extern "C" void port_test_set_next_sync0(uint64_t t) { g_next_sync0 = t; }

extern "C" void port_test_set_dc_sys_time(uint64_t t) { g_dc_sys_time = t; }

extern "C" void port_test_fpga_reset() {
  std::memset(g_ctl, 0, sizeof(g_ctl));
  std::memset(g_phase_corr, 0, sizeof(g_phase_corr));
  std::memset(g_output_mask, 0, sizeof(g_output_mask));
  std::memset(g_pwe, 0, sizeof(g_pwe));
  std::memset(g_mod_ram, 0, sizeof(g_mod_ram));
  std::memset(g_em_ram, 0, sizeof(g_em_ram));
  std::memset(g_latch_count, 0, sizeof(g_latch_count));
  g_next_sync0 = 0;
  g_dc_sys_time = 0;
}

extern "C" uint16_t port_test_fpga_ctl(uint16_t addr) { return g_ctl[addr & 0xFF]; }

extern "C" uint16_t port_test_fpga_phase_corr(uint16_t idx) { return g_phase_corr[idx & 0xFF]; }

extern "C" uint16_t port_test_fpga_output_mask(uint16_t idx) { return g_output_mask[idx & (OUTPUT_MASK_WORDS - 1)]; }

extern "C" uint16_t port_test_fpga_pwe_word(uint16_t idx) { return g_pwe[idx & (PWE_TABLE_SIZE - 1)]; }

extern "C" uint32_t port_test_fpga_latch_count(uint16_t flag) {
  for (int bit = 0; bit < 16; ++bit) {
    if ((flag & (1u << bit)) != 0) return g_latch_count[bit];
  }
  return 0;
}

extern "C" uint16_t port_test_fpga_mod_word(uint8_t bank, uint32_t word_idx) { return g_mod_ram[bank][word_idx]; }

extern "C" uint16_t port_test_fpga_emission_word(uint8_t bank, uint32_t word_idx) { return g_em_ram[bank][word_idx]; }
