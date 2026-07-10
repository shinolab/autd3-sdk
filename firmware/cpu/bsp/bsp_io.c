#include "bsp.h"

#include "rzt1_regs.h"

void bsp_io_init(void) {
  volatile uint8_t dummy;

  /* Enable writing to the PmnPFS registers */
  MPC_PWPR = 0x00;
  dummy = MPC_PWPR;
  MPC_PWPR = 0x40;
  dummy = MPC_PWPR;

  /* PORT5 setting */
  RZT1_PDR_SET(PORT5_PDR, 6u, 3u);

  /* PORTA setting */
  RZT1_PDR_SET(PORTA_PDR, 4u, 0u);
  PORTA_PMR &= (uint8_t)~(1u << 4);
  RZT1_PDR_SET(PORTA_PDR, 5u, 3u);
  PORTA_PMR &= (uint8_t)~(1u << 5);
  RZT1_PDR_SET(PORTA_PDR, 6u, 0u);
  PORTA_PMR &= (uint8_t)~(1u << 6);
  RZT1_PDR_SET(PORTA_PDR, 7u, 3u);
  PORTA_PMR &= (uint8_t)~(1u << 7);

  /* PORTF setting */
  RZT1_PDR_SET(PORTF_PDR, 7u, 3u);

  /* PORTN setting (LED1 / LED2) */
  PORTN_PODR |= (uint8_t)(1u << 6);
  PORTN_PODR |= (uint8_t)(1u << 7);
  RZT1_PDR_SET(PORTN_PDR, 6u, 3u);
  RZT1_PDR_SET(PORTN_PDR, 7u, 3u);

  /* Disable writing to the PmnPFS registers */
  MPC_PWPR = 0x80;
  dummy = MPC_PWPR;
  (void)dummy;
}
