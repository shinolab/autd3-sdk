#ifndef RZT1_REGS_H_
#define RZT1_REGS_H_

#include <stdint.h>

#define RZT1_REG8(addr) (*(volatile uint8_t*)(addr))
#define RZT1_REG16(addr) (*(volatile uint16_t*)(addr))
#define RZT1_REG32(addr) (*(volatile uint32_t*)(addr))
#define RZT1_REG64(addr) (*(volatile uint64_t*)(addr))

/* ---- System control (clock generator, low power) ---- */

#define SYSTEM_SCKCR RZT1_REG32(0xA00B0020u)   /* system clock control */
#define SYSTEM_SCKCR2 RZT1_REG32(0xA00B0024u)  /* system clock control 2 */
#define SYSTEM_PLL1CR RZT1_REG32(0xA00B0034u)  /* PLL1 control */
#define SYSTEM_PLL1CR2 RZT1_REG32(0xA00B0038u) /* PLL1 control 2 */
#define SYSTEM_LOCOCR RZT1_REG32(0xA00B0040u)  /* low-speed on-chip oscillator */
#define SYSTEM_MSTPCRA RZT1_REG32(0xA00B0300u) /* module stop control A */
#define SYSTEM_MSTPCRC RZT1_REG32(0xA00B0308u) /* module stop control C */
#define SYSTEM_PRCR RZT1_REG32(0xA00B0B00u)    /* protect register */

#define SYSTEM_LOCOCR_LCSTP (1uL << 0)
#define SYSTEM_SCKCR_CKIO_SHIFT (8u)
#define SYSTEM_SCKCR_CKIO_MASK (7uL << SYSTEM_SCKCR_CKIO_SHIFT)

/* ---- VIC (interrupt controller) ---- */

#define VIC_IEN0 RZT1_REG32(0xA0010080u) /* interrupt enable (int 0-31) */
#define VIC_IEC0 RZT1_REG32(0xA00100A0u) /* interrupt enable clear (int 0-31) */
#define VIC_IEC1 RZT1_REG32(0xA00100A4u)
#define VIC_IEC2 RZT1_REG32(0xA00100A8u)
#define VIC_IEC3 RZT1_REG32(0xA00100ACu)
#define VIC_IEC4 RZT1_REG32(0xA00100B0u)
#define VIC_IEC5 RZT1_REG32(0xA00100B4u)
#define VIC_IEC6 RZT1_REG32(0xA00100B8u)
#define VIC_IEC7 RZT1_REG32(0xA00100BCu)
#define VIC_IEC8 RZT1_REG32(0xA00110A0u) /* int 256-287 (second bank) */
#define VIC_IEC9 RZT1_REG32(0xA00110A4u)
#define VIC_PLS0 RZT1_REG32(0xA0010100u) /* detection type: edge (int 0-31) */
#define VIC_PIC0 RZT1_REG32(0xA0010120u) /* interrupt request clear (int 0-31) */
#define VIC_HVA0 RZT1_REG32(0xA0010200u) /* hardware vector address (EOI) */

/* Per-interrupt vector address / priority registers (int 1-255). */
#define VIC_VADn(n) RZT1_REG32(0xA0010400u + 4u * (n))
#define VIC_PRLn(n) RZT1_REG32(0xA0010800u + 4u * (n))

/* ---- CMT (compare match timer, unit 0) ---- */

#define CMT_CMSTR0 RZT1_REG16(0xA0080000u) /* start (STR0 = bit0, STR1 = bit1) */
#define CMT0_CMCR RZT1_REG16(0xA0080002u)  /* control */
#define CMT0_CMCNT RZT1_REG16(0xA0080004u) /* counter */
#define CMT0_CMCOR RZT1_REG16(0xA0080006u) /* compare match constant */

#define CMT_CMSTR0_STR0 (1u << 0)
#define CMT0_CMCR_CKS_MASK (3u << 0) /* 0 = PCLKD/8 */
#define CMT0_CMCR_CMIE (1u << 6)

/* ---- Port / pin function control ---- */

/* PDR: direction, 16-bit, 2 bits per pin. PODR: output data, 8-bit, 1 bit
 * per pin. PMR: mode (0 = I/O port, 1 = peripheral), 8-bit, 1 bit per pin.
 * DSCR: drive strength, 16-bit access. */
#define PORT5_PDR RZT1_REG16(0xA000000Au)
#define PORTA_PDR RZT1_REG16(0xA0000014u)
#define PORTF_PDR RZT1_REG16(0xA000001Eu)
#define PORTN_PDR RZT1_REG16(0xA000002Cu)
#define PORTN_PODR RZT1_REG8(0xA0000056u)
#define PORT0_PMR RZT1_REG8(0xA0000080u)
#define PORT1_PMR RZT1_REG8(0xA0000081u)
#define PORT2_PMR RZT1_REG8(0xA0000082u)
#define PORT3_PMR RZT1_REG8(0xA0000083u)
#define PORT4_PMR RZT1_REG8(0xA0000084u)
#define PORT9_PMR RZT1_REG8(0xA0000089u)
#define PORTA_PMR RZT1_REG8(0xA000008Au)
#define PORTE_PMR RZT1_REG8(0xA000008Eu)
#define PORTG_PMR RZT1_REG8(0xA0000090u)
#define PORTH_PMR RZT1_REG8(0xA0000091u)
#define PORTK_PMR RZT1_REG8(0xA0000093u)
#define PORT1_DSCR RZT1_REG16(0xA0000142u)

/* Sets a 2-bit PDR field for a pin (reg is a PORTn_PDR above). */
#define RZT1_PDR_SET(reg, pin, val) \
  ((reg) = (uint16_t)(((reg) & ~(3u << (2u * (pin)))) | ((uint16_t)(val) << (2u * (pin)))))

/* MPC: pin function select (8-bit each) and its write protect. */
#define MPC_P00PFS RZT1_REG8(0xA0000200u)
#define MPC_P01PFS RZT1_REG8(0xA0000201u)
#define MPC_P02PFS RZT1_REG8(0xA0000202u)
#define MPC_P03PFS RZT1_REG8(0xA0000203u)
#define MPC_P04PFS RZT1_REG8(0xA0000204u)
#define MPC_P05PFS RZT1_REG8(0xA0000205u)
#define MPC_P06PFS RZT1_REG8(0xA0000206u)
#define MPC_P07PFS RZT1_REG8(0xA0000207u)
#define MPC_P10PFS RZT1_REG8(0xA0000208u)
#define MPC_P15PFS RZT1_REG8(0xA000020Du)
#define MPC_P24PFS RZT1_REG8(0xA0000214u)
#define MPC_P36PFS RZT1_REG8(0xA000021Eu)
#define MPC_P37PFS RZT1_REG8(0xA000021Fu)
#define MPC_P46PFS RZT1_REG8(0xA0000226u)
#define MPC_P90PFS RZT1_REG8(0xA0000248u)
#define MPC_PE0PFS RZT1_REG8(0xA0000270u)
#define MPC_PE1PFS RZT1_REG8(0xA0000271u)
#define MPC_PE2PFS RZT1_REG8(0xA0000272u)
#define MPC_PE3PFS RZT1_REG8(0xA0000273u)
#define MPC_PE4PFS RZT1_REG8(0xA0000274u)
#define MPC_PE5PFS RZT1_REG8(0xA0000275u)
#define MPC_PE6PFS RZT1_REG8(0xA0000276u)
#define MPC_PE7PFS RZT1_REG8(0xA0000277u)
#define MPC_PG0PFS RZT1_REG8(0xA0000280u)
#define MPC_PG1PFS RZT1_REG8(0xA0000281u)
#define MPC_PG2PFS RZT1_REG8(0xA0000282u)
#define MPC_PG3PFS RZT1_REG8(0xA0000283u)
#define MPC_PG4PFS RZT1_REG8(0xA0000284u)
#define MPC_PG5PFS RZT1_REG8(0xA0000285u)
#define MPC_PG6PFS RZT1_REG8(0xA0000286u)
#define MPC_PG7PFS RZT1_REG8(0xA0000287u)
#define MPC_PH0PFS RZT1_REG8(0xA0000288u)
#define MPC_PH1PFS RZT1_REG8(0xA0000289u)
#define MPC_PH2PFS RZT1_REG8(0xA000028Au)
#define MPC_PH3PFS RZT1_REG8(0xA000028Bu)
#define MPC_PH4PFS RZT1_REG8(0xA000028Cu)
#define MPC_PH5PFS RZT1_REG8(0xA000028Du)
#define MPC_PH6PFS RZT1_REG8(0xA000028Eu)
#define MPC_PH7PFS RZT1_REG8(0xA000028Fu)
#define MPC_PK0PFS RZT1_REG8(0xA0000298u)
#define MPC_PWPR RZT1_REG8(0xA00002FFu)

/* ---- EtherCAT slave controller (distributed clocks) ---- */

#define ECATC_DC_SYS_TIME RZT1_REG64(0xA00D0910u)
#define ECATC_DC_CYC_START_TIME RZT1_REG64(0xA00D0990u)

#endif /* RZT1_REGS_H_ */
