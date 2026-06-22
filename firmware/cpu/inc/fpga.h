#ifndef INC_FPGA_H_
#define INC_FPGA_H_

#include <stdint.h>

#include "proto.h"

#ifdef __cplusplus
extern "C" {
#endif

#define FPGA_PAGE_WORDS (16384UL)

#define TRANSITION_MODE_IMMEDIATE (0xFF)
#define REP_INFINITE (0xFFFF)

#define PWE_TABLE_SIZE (256)
#define SILENCER_DEFAULT_UPDATE_RATE (256)
#define SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY (10)
#define SILENCER_DEFAULT_COMPLETION_STEPS_PHASE (40)
#define OUTPUT_MASK_WORDS (32)
#define PHASE_CORR_WORDS ((NUM_TRANSDUCERS + 1) / 2)
#define DEBUG_VALUE_WORDS (16)

void fpga_write(uint8_t select, uint16_t addr, uint16_t value);
uint16_t fpga_read(uint8_t select, uint16_t addr);
void set_and_wait_update(uint16_t flag);
void fpga_write_u64(uint16_t addr, uint64_t value);
void fpga_write_change_bank(uint16_t req_rd_bank_addr, uint16_t transition_mode_addr, uint16_t transition_value_addr,
                            uint8_t bank, uint8_t transition_mode, uint64_t transition_value);
void fpga_write_ram(uint8_t select, uint16_t wr_bank_reg, uint16_t wr_page_reg, uint8_t bank, uint32_t offset,
                    const uint8_t* src, uint16_t len_bytes);
void fpga_init(void);

#ifdef __cplusplus
}
#endif

#endif /* INC_FPGA_H_ */
