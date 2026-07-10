#include "bsp.h"

#include "rzt1_regs.h"

#define PRCR_CPG_UNLOCK (0x0000A501uL)
#define PRCR_CPG_LOCK (0x0000A500uL)
#define PLL1CR_CPUCKSEL_600_MHZ (3uL)
#define SCKCR_CKIO_75_MHZ (0uL)

void bsp_clock_init(void) {
  volatile uint32_t dummy;
  volatile uint32_t loop;

  SYSTEM_PRCR = PRCR_CPG_UNLOCK;
  dummy = SYSTEM_PRCR;

  /* Enable LOCO clock operation */
  SYSTEM_LOCOCR &= ~SYSTEM_LOCOCR_LCSTP;

  /* Select 600 MHz CPU clock (dummy read three times to settle the value) */
  SYSTEM_PLL1CR = PLL1CR_CPUCKSEL_600_MHZ;
  dummy = SYSTEM_PLL1CR;
  dummy = SYSTEM_PLL1CR;
  dummy = SYSTEM_PLL1CR;

  /* Enable PLL1 and wait about 100us for stabilization */
  SYSTEM_PLL1CR2 = 1uL;
  for (loop = 0u; loop < 20000u; loop++) {
    __asm__ volatile("nop");
  }

  /* Select PLL1 as the clock source */
  SYSTEM_SCKCR2 = 1uL;

  /* Set BSC CKIO clock to 75 MHz */
  SYSTEM_SCKCR = (SYSTEM_SCKCR & ~SYSTEM_SCKCR_CKIO_MASK) |
                 (SCKCR_CKIO_75_MHZ << SYSTEM_SCKCR_CKIO_SHIFT);

  SYSTEM_PRCR = PRCR_CPG_LOCK;
  dummy = SYSTEM_PRCR;
  (void)dummy;
}
