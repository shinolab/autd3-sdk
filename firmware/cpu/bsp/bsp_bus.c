#include "bsp.h"
#include "rzt1_regs.h"

void bsp_bus_init(void) {
  volatile uint32_t dummy;
  volatile uint8_t dummy8;

  /* Enable writing to the PmnPFS registers */
  MPC_PWPR = 0x00;
  dummy8 = MPC_PWPR;
  MPC_PWPR = 0x40;
  dummy8 = MPC_PWPR;

  MPC_P00PFS = 0x22; /* P00 D0 */
  MPC_P01PFS = 0x22; /* P01 D1 */
  MPC_P02PFS = 0x22; /* P02 D2 */
  MPC_P03PFS = 0x22; /* P03 D3 */
  MPC_P04PFS = 0x22; /* P04 D4 */
  MPC_P05PFS = 0x22; /* P05 D5 */
  MPC_P06PFS = 0x22; /* P06 D6 */
  MPC_P07PFS = 0x22; /* P07 D7 */

  MPC_P10PFS = 0x22; /* P10 CKIO */
  MPC_P15PFS = 0x22; /* P15 CS3# */

  MPC_P24PFS = 0x22; /* P24 RD/WR# */

  MPC_P36PFS = 0x22; /* P36 DQMLL */
  MPC_P37PFS = 0x22; /* P37 DQMLU */

  MPC_P46PFS = 0x22; /* P46 CKE */

  MPC_P90PFS = 0x23; /* P90 RAS# */

  MPC_PE0PFS = 0x22; /* PE0 D8 */
  MPC_PE1PFS = 0x22; /* PE1 D9 */
  MPC_PE2PFS = 0x22; /* PE2 D10 */
  MPC_PE3PFS = 0x22; /* PE3 D11 */
  MPC_PE4PFS = 0x22; /* PE4 D12 */
  MPC_PE5PFS = 0x22; /* PE5 D13 */
  MPC_PE6PFS = 0x22; /* PE6 D14 */
  MPC_PE7PFS = 0x22; /* PE7 D15 */

  MPC_PG0PFS = 0x22; /* PG0 A1 */
  MPC_PG1PFS = 0x22; /* PG1 A2 */
  MPC_PG2PFS = 0x22; /* PG2 A3 */
  MPC_PG3PFS = 0x22; /* PG3 A4 */
  MPC_PG4PFS = 0x22; /* PG4 A5 */
  MPC_PG5PFS = 0x22; /* PG5 A6 */
  MPC_PG6PFS = 0x22; /* PG6 A7 */
  MPC_PG7PFS = 0x22; /* PG7 A8 */

  MPC_PH0PFS = 0x22; /* PH0 A9 */
  MPC_PH1PFS = 0x22; /* PH1 A10 */
  MPC_PH2PFS = 0x22; /* PH2 A11 */
  MPC_PH3PFS = 0x22; /* PH3 A12 */
  MPC_PH4PFS = 0x22; /* PH4 A13 */
  MPC_PH5PFS = 0x22; /* PH5 A14 */
  MPC_PH6PFS = 0x22; /* PH6 A15 */
  MPC_PH7PFS = 0x22; /* PH7 A16 */

  MPC_PK0PFS = 0x23; /* PK0 CAS# */

  /* Disable writing to the PmnPFS registers */
  MPC_PWPR = 0x80;
  (void)dummy8;

  PORT0_PMR = 0xFF;
  PORT1_PMR = 0x21;
  PORT2_PMR = 0x10;
  PORT3_PMR = 0xD8;
  PORT4_PMR = 0x40;
  PORT9_PMR = 0x01;
  PORTE_PMR = 0xFF;
  PORTG_PMR = 0xFF;
  PORTH_PMR = 0xFF;
  PORTK_PMR = 0x01;

  PORT1_DSCR = 0x0001;

  /* Release the external bus modules (BSC) from the module stop state */
  SYSTEM_PRCR = 0x0000A502uL;
  dummy = SYSTEM_PRCR;
  SYSTEM_MSTPCRC = 0x00007C7EuL;
  dummy = SYSTEM_MSTPCRC;
  SYSTEM_PRCR = 0x0000A500uL;
  dummy = SYSTEM_PRCR;
  (void)dummy;
}
