#ifndef BSP_H_
#define BSP_H_

#include <stdint.h>

#define BSP_INT_CPUINT (1u)

void bsp_bus_init(void);
void bsp_clock_init(void);
void bsp_io_init(void);
void bsp_vic_init(void);
void bsp_timer_init(void);

void bsp_vic_install(uint32_t intno, uint32_t priority, void (*handler)(void));
void bsp_irq_enable(void);

void bsp_delay_ms(uint16_t ms);

#endif /* BSP_H_ */
