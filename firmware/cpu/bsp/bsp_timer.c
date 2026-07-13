#include "bsp.h"

#include "rzt1_regs.h"

/* CMT unit 0 is clocked from PCLKD (fixed 75 MHz), divided by 8 (CKS = 0). */
#define CMT0_PCLKD_HZ (75000000uL)
#define CMT0_CLOCK_DIVIDER (8uL)
#define CMT0_TICKS_PER_MS ((CMT0_PCLKD_HZ / CMT0_CLOCK_DIVIDER) / 1000uL)

/* MSTPCRA bit 4 stops CMT unit 0. */
#define MSTPCRA_CMT_UNIT0 (0x00000010uL)

/* PRCR unlock/lock for the low-power (module stop) registers. */
#define PRCR_LPC_UNLOCK (0x0000A502uL)
#define PRCR_LPC_LOCK (0x0000A500uL)

void bsp_timer_init(void) {
  volatile uint32_t dummy;

  /* Cancel CMT unit 0 stop state in LPC. */
  SYSTEM_PRCR = PRCR_LPC_UNLOCK;
  dummy = SYSTEM_PRCR;
  SYSTEM_MSTPCRA &= ~MSTPCRA_CMT_UNIT0;
  dummy = SYSTEM_MSTPCRA;
  SYSTEM_PRCR = PRCR_LPC_LOCK;
  dummy = SYSTEM_PRCR;
  (void)dummy;

  /* Free-running counter: PCLKD / 8, no compare-match interrupt. CMCOR at its
   * maximum makes CMCNT wrap modulo 2^16, so unsigned 16-bit differences give
   * the elapsed tick count as long as it is sampled faster than the 7 ms
   * period. */
  CMT_CMSTR0 &= (uint16_t)~CMT_CMSTR0_STR0;
  CMT0_CMCR &= (uint16_t)~(CMT0_CMCR_CKS_MASK | CMT0_CMCR_CMIE);
  CMT0_CMCOR = 0xFFFFu;
  CMT0_CMCNT = 0u;
  CMT_CMSTR0 |= CMT_CMSTR0_STR0;
}

void bsp_delay_ms(uint16_t ms) {
  uint32_t remaining = (uint32_t)ms * CMT0_TICKS_PER_MS;
  uint16_t prev = CMT0_CMCNT;

  while (remaining != 0u) {
    const uint16_t now = CMT0_CMCNT;
    const uint16_t elapsed = (uint16_t)(now - prev);
    prev = now;
    remaining = ((uint32_t)elapsed >= remaining) ? 0u : (remaining - (uint32_t)elapsed);
  }
}
