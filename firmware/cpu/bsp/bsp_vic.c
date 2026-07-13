#include "bsp.h"

#include "rzt1_regs.h"

void bsp_vic_init(void) {
  /* Disable and clear all interrupt sources. SCTLR.VE is set by the loader,
   * so the VIC supplies handler addresses (VADn) directly to the core. */
  VIC_IEC0 = 0xFFFFFFFFu;
  VIC_IEC1 = 0xFFFFFFFFu;
  VIC_IEC2 = 0xFFFFFFFFu;
  VIC_IEC3 = 0xFFFFFFFFu;
  VIC_IEC4 = 0xFFFFFFFFu;
  VIC_IEC5 = 0xFFFFFFFFu;
  VIC_IEC6 = 0xFFFFFFFFu;
  VIC_IEC7 = 0xFFFFFFFFu;
  VIC_IEC8 = 0xFFFFFFFFu;
  VIC_IEC9 = 0xFFFFFFFFu;
}

/* Interrupts are masked from reset (startup.S leaves CPSR.I set) and stay
 * masked until the application has installed its handlers. */
void bsp_irq_enable(void) { __asm__ volatile("cpsie i" ::: "memory"); }

void bsp_vic_install(uint32_t intno, uint32_t priority, void (*handler)(void)) {
  if ((intno == 0u) || (intno > 31u)) {
    for (;;) {
    }
  }

  VIC_IEC0 = (1uL << intno);       /* disable while configuring */
  VIC_PLS0 |= (1uL << intno);      /* edge detection */
  VIC_PRLn(intno) = priority;      /* 0 (highest) to 31 (lowest) */
  VIC_VADn(intno) = (uint32_t)handler;
  VIC_PIC0 = (1uL << intno);       /* clear stale request */
  VIC_IEN0 |= (1uL << intno);
}
