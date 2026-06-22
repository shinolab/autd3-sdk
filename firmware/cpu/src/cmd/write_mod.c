#ifdef __cplusplus
extern "C" {
#endif

#include "cmd/write_mod.h"

#include <stdint.h>

#include "fpga.h"
#include "proto.h"

uint8_t write_mod_handle(const uint8_t* payload) {
  const write_mod_payload_t* p = (const write_mod_payload_t*)payload;
  if ((p->bank >= NUM_BANKS) || ((p->offset % 2u) != 0u) || (p->data_len > MOD_WRITE_MAX_DATA_LEN) ||
      (p->offset > MOD_BUFFER_SAMPLES) || ((uint32_t)p->data_len > MOD_BUFFER_SAMPLES - p->offset)) {
    return ERR_INVALID_PAYLOAD;
  }
  fpga_write_ram(BRAM_SELECT_MOD, ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, p->bank, p->offset / 2u, p->data,
                 p->data_len);
  return ERR_NONE;
}

#ifdef __cplusplus
}
#endif
