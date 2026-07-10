#ifndef FREERTOS_CONFIG_H
#define FREERTOS_CONFIG_H

#define configCPU_CLOCK_HZ (600000000uL)
#define configTICK_RATE_HZ ((TickType_t)1000)
#define configUSE_PREEMPTION 1
#define configUSE_PORT_OPTIMISED_TASK_SELECTION 1
#define configUSE_TICKLESS_IDLE 0
#define configMAX_PRIORITIES (4)
#define configMINIMAL_STACK_SIZE ((unsigned short)256)
#define configTOTAL_HEAP_SIZE ((size_t)(16 * 1024))
#define configMAX_TASK_NAME_LEN (8)
#define configUSE_16_BIT_TICKS 0
#define configIDLE_SHOULD_YIELD 1
#define configUSE_TASK_NOTIFICATIONS 0
#define configUSE_MUTEXES 0
#define configUSE_RECURSIVE_MUTEXES 0
#define configUSE_COUNTING_SEMAPHORES 0
#define configQUEUE_REGISTRY_SIZE 0
#define configUSE_QUEUE_SETS 0
#define configUSE_TIME_SLICING 1
#define configUSE_APPLICATION_TASK_TAG 0
#define configUSE_NEWLIB_REENTRANT 0
#define configENABLE_BACKWARD_COMPATIBILITY 0

#define configUSE_IDLE_HOOK 0
#define configUSE_TICK_HOOK 0
#define configCHECK_FOR_STACK_OVERFLOW 2
#define configUSE_MALLOC_FAILED_HOOK 1

#define configGENERATE_RUN_TIME_STATS 0
#define configUSE_TRACE_FACILITY 0
#define configUSE_STATS_FORMATTING_FUNCTIONS 0

#define configUSE_CO_ROUTINES 0
#define configUSE_TIMERS 0

#define INCLUDE_vTaskPrioritySet 0
#define INCLUDE_uxTaskPriorityGet 0
#define INCLUDE_vTaskDelete 0
#define INCLUDE_vTaskSuspend 0
#define INCLUDE_vTaskDelayUntil 0
#define INCLUDE_vTaskDelay 1
#define INCLUDE_xTaskGetSchedulerState 0

#define configEOI_ADDRESS 0xA0010200uL

#define configCLEAR_TICK_INTERRUPT() (VIC_PIC0 = (1uL << 21))

#define configSETUP_TICK_INTERRUPT() vConfigureTickInterrupt()

#ifndef __ASSEMBLER__

#include "rzt1_regs.h"

void vConfigureTickInterrupt(void);

typedef void (*ISRFunction_t)(void);

#define configASSERT(x)       \
  if ((x) == 0) {             \
    portDISABLE_INTERRUPTS(); \
    for (;;) {                \
    }                         \
  }

#endif /* __ASSEMBLER__ */

#endif /* FREERTOS_CONFIG_H */
