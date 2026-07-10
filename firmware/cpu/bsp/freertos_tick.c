/*
 * FreeRTOS tick configuration for RZ/T1 using CMT unit 0 channel 0.
 *
 * Adapted from FreeRTOS V202212.00 Demo/CORTEX_R4F_RZ_T_GCC_IAR/src/FreeRTOS_tick_config.c
 * Copyright (C) 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 *
 * https://www.FreeRTOS.org
 * https://github.com/FreeRTOS
 */

#include "FreeRTOS.h"

#include "bsp.h"
#include "rzt1_regs.h"

/* CMT unit 0 is clocked from PCLKD (fixed 75 MHz), divided by 8 (CKS = 0). */
#define CMT0_PCLKD_HZ (75000000uL)
#define CMT0_CLOCK_DIVIDER (8uL)

/* MSTPCRA bit 4 stops CMT unit 0. */
#define MSTPCRA_CMT_UNIT0 (0x00000010uL)

/* PRCR unlock/lock for the low-power (module stop) registers. */
#define PRCR_LPC_UNLOCK (0x0000A502uL)
#define PRCR_LPC_LOCK (0x0000A500uL)

/*
 * Entry point for the FreeRTOS tick interrupt. This sets the pxISRFunction
 * variable to point to the RTOS tick handler, then branches to the FreeRTOS
 * IRQ handler. NOTE: this is a naked function - do not add C code to it.
 */
static void FreeRTOS_Tick_Handler_Entry(void) __attribute__((naked));

/* The FreeRTOS IRQ handler, implemented in the RTOS port layer (portASM.S). */
extern void FreeRTOS_IRQ_Handler(void);

/* The FreeRTOS tick handler, implemented in the RTOS port layer (port.c). */
extern void FreeRTOS_Tick_Handler(void);

/*
 * Variable used to hold the address of the interrupt handler the FreeRTOS IRQ
 * handler will branch to.
 */
ISRFunction_t pxISRFunction = NULL;

/*
 * Called by xPortStartScheduler() via configSETUP_TICK_INTERRUPT() to
 * configure CMT0 as the tick interrupt source.
 */
void vConfigureTickInterrupt(void) {
  uint32_t ulCompareMatchValue;
  volatile uint32_t ulDummy;

  ulCompareMatchValue = (CMT0_PCLKD_HZ / CMT0_CLOCK_DIVIDER) / configTICK_RATE_HZ;
  ulCompareMatchValue -= 1uL;

  /* Cancel CMT unit 0 stop state in LPC. */
  SYSTEM_PRCR = PRCR_LPC_UNLOCK;
  ulDummy = SYSTEM_PRCR;
  SYSTEM_MSTPCRA &= ~MSTPCRA_CMT_UNIT0;
  ulDummy = SYSTEM_MSTPCRA;
  SYSTEM_PRCR = PRCR_LPC_LOCK;
  ulDummy = SYSTEM_PRCR;
  (void)ulDummy;

  /* Interrupt on compare match, PCLKD / 8. */
  CMT0_CMCR |= CMT0_CMCR_CMIE;
  CMT0_CMCOR = (uint16_t)ulCompareMatchValue;
  CMT0_CMCR &= (uint16_t)~CMT0_CMCR_CKS_MASK;
  CMT0_CMCNT = 0;

  /* Install the tick handler at the lowest priority and start the count. */
  bsp_vic_install(BSP_INT_CMI0, 31u, FreeRTOS_Tick_Handler_Entry);
  CMT_CMSTR0 |= CMT_CMSTR0_STR0;
}

/*
 * The function called by the FreeRTOS IRQ handler, after it has managed
 * interrupt entry. This function creates a local copy of pxISRFunction before
 * re-enabling interrupts and actually calling the handler pointed to by
 * pxISRFunction.
 */
void vApplicationIRQHandler(void) {
  ISRFunction_t pxISRToCall = pxISRFunction;

  portENABLE_INTERRUPTS();

  pxISRToCall();
}

/*
 * The RZ/T VIC vectors directly to a peripheral specific interrupt handler
 * (VADn), rather than using the Cortex-R IRQ vector. Therefore each interrupt
 * handler installed by the application that uses FreeRTOS API functions must
 * follow this example: save a pointer to a standard C function in the
 * pxISRFunction variable, then branch to the FreeRTOS IRQ handler, which
 * manages interrupt entry (including nesting) before calling the C function.
 */
static void FreeRTOS_Tick_Handler_Entry(void) {
  __asm volatile("PUSH  {r0-r1}                       \t\n"
                 "LDR   r0, =pxISRFunction            \t\n"
                 "LDR   r1, =FreeRTOS_Tick_Handler    \t\n"
                 "STR   r1, [r0]                      \t\n"
                 "POP   {r0-r1}                       \t\n"
                 "B     FreeRTOS_IRQ_Handler            ");
}
