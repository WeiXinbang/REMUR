# rvmodel_macros.h
# DUT-specific macro definitions for REMUR (RISC-V Emulator in Rust)
# SPDX-License-Identifier: Apache-2.0

#ifndef _RVMODEL_MACROS_H
#define _RVMODEL_MACROS_H

#define RVMODEL_DATA_SECTION \
        .pushsection .tohost,"aw",@progbits;                \
        .align 8; .global tohost; tohost: .dword 0;         \
        .align 8; .global fromhost; fromhost: .dword 0;     \
        .popsection

##### STARTUP #####

//#define RVMODEL_BOOT

##### TERMINATION #####

#define RVMODEL_HALT_PASS  \
  li x1, 1                ;\
  la t0, tohost           ;\
  write_tohost_pass:      ;\
    sw x1, 0(t0)          ;\
    sw x0, 4(t0)          ;\
    j write_tohost_pass   ;\

#define RVMODEL_HALT_FAIL \
  li x1, 3                ;\
  la t0, tohost           ;\
  write_tohost_fail:      ;\
    sw x1, 0(t0)          ;\
    sw x0, 4(t0)          ;\
    j write_tohost_fail   ;\

##### IO #####

# REMUR UART at 0x10000000 (PC16550 compatible)
.EQU UART_BASE_ADDR, 0x10000000
.EQU UART_THR, (UART_BASE_ADDR + 0)
.EQU UART_LCR, (UART_BASE_ADDR + 3)
.EQU UART_LSR, (UART_BASE_ADDR + 5)

#define RVMODEL_IO_INIT(_R1, _R2, _R3)    \
  uart_init:                ;\
    li _R1, UART_LCR         ;\
    li _R2, 3                ;\
    sb _R2, 0(_R1)           ;\

#define RVMODEL_IO_WRITE_STR(_R1, _R2, _R3, _STR_PTR)               \
1:                           ;                       \
  lbu _R1, 0(_STR_PTR)        ;\
  beqz _R1, 3f                ;\
2:                           ;\
  li _R2, UART_LSR ;\
  4: \
    lbu _R3, 0(_R2) ;\
    andi _R3, _R3, 0x20 ;\
    beqz _R3, 4b ;\
    li _R2, UART_THR ;\
    sb _R1, 0(_R2) ;\
  addi _STR_PTR, _STR_PTR, 1 ;\
  j 1b                       ;\
3:

##### Access Fault #####

#define RVMODEL_ACCESS_FAULT_ADDRESS 0x00000000

##### Machine Timer #####

#define RVMODEL_MTIME_ADDRESS  0x0200BFF8
#define RVMODEL_MTIMECMP_ADDRESS 0x02004000

##### Machine Interrupts #####

#define RVMODEL_INTERRUPT_LATENCY 10
#define RVMODEL_TIMER_INT_SOON_DELAY 100

#define CLINT_BASE_ADDRESS 0x02000000
#define MSIP_ADDRESS (CLINT_BASE_ADDRESS + 0x0)

#define RVMODEL_SET_MEXT_INT(_R1, _R2)
#define RVMODEL_CLR_MEXT_INT(_R1, _R2)

#define RVMODEL_SET_MSW_INT(_R1, _R2) \
  li _R1, 1; \
  li _R2, MSIP_ADDRESS; \
  sw _R1, 0(_R2);

#define RVMODEL_CLR_MSW_INT(_R1, _R2) \
  li _R2, MSIP_ADDRESS; \
  sw zero, 0(_R2);

##### Supervisor Interrupts #####

#define RVMODEL_SET_SEXT_INT(_R1, _R2)
#define RVMODEL_CLR_SEXT_INT(_R1, _R2)
#define RVMODEL_SET_SSW_INT(_R1, _R2)
#define RVMODEL_CLR_SSW_INT(_R1, _R2)

#endif // _RVMODEL_MACROS_H
