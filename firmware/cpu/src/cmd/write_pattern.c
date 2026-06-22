#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/write_pattern.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t write_pattern_handle(const uint8_t* payload) {
  const write_pattern_payload_t* p = (const write_pattern_payload_t*)payload;
  if ((p->bank >= NUM_BANKS) || ((p->data_len % 2u) != 0u) || (p->data_len > EM_WRITE_MAX_DATA_LEN) ||
      (p->offset > EMISSION_RAM_WORDS) || ((uint32_t)(p->data_len / 2u) > EMISSION_RAM_WORDS - p->offset)) {
    return ERR_INVALID_PAYLOAD;
  }
  fpga_write_ram(BRAM_SELECT_EMISSION, ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, p->bank, p->offset, p->data,
                 p->data_len);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
