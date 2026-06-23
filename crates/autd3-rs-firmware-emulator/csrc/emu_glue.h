#ifndef EMU_GLUE_H_
#define EMU_GLUE_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void* emu_device_new(void);

void emu_device_free(void* handle);

void emu_device_select(void* handle);

uint8_t emu_tx_ack(void);
uint8_t emu_tx_data(void);

#ifdef __cplusplus
}
#endif

#endif /* EMU_GLUE_H_ */
