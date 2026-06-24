#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/write_pattern_compressed.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t write_pattern_compressed_handle(const uint8_t* payload) {
  const write_pattern_compressed_payload_t* p = (const write_pattern_compressed_payload_t*)payload;
  const uint8_t max_count = (p->format == WRITE_PATTERN_FORMAT_PHASE_FULL) ? 2u : 4u;
  uint8_t slot[NUM_TRANSDUCERS * 2u];
  uint8_t g;
  uint16_t t;

  if ((p->bank >= NUM_BANKS) || (p->format < WRITE_PATTERN_FORMAT_PHASE_FULL) ||
      (p->format > WRITE_PATTERN_FORMAT_PHASE_HALF) || (p->count < 1u) || (p->count > max_count) ||
      (p->offset > EMISSION_RAM_WORDS) ||
      ((uint32_t)(p->count - 1u) * EMISSION_SLOT_WORDS + NUM_TRANSDUCERS > EMISSION_RAM_WORDS - p->offset)) {
    return ERR_INVALID_PAYLOAD;
  }

  for (g = 0u; g < p->count; g++) {
    for (t = 0u; t < NUM_TRANSDUCERS; t++) {
      const uint16_t w = (uint16_t)((uint16_t)p->data[2u * t] | ((uint16_t)p->data[2u * t + 1u] << 8));
      uint8_t phase;
      if (p->format == WRITE_PATTERN_FORMAT_PHASE_FULL) {
        phase = (uint8_t)((w >> (8u * g)) & 0xFFu);
      } else {
        const uint8_t p4 = (uint8_t)((w >> (4u * g)) & 0x0Fu);
        phase = (uint8_t)((p4 << 4) | p4);
      }
      slot[2u * t] = phase;
      slot[2u * t + 1u] = 0xFFu;
    }
    fpga_write_ram(BRAM_SELECT_EMISSION, ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, p->bank,
                   p->offset + (uint32_t)g * EMISSION_SLOT_WORDS, slot, (uint16_t)(NUM_TRANSDUCERS * 2u));
  }
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
