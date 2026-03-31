#include <stdint.h>
#include <stdlib.h>
#include <string.h>

// 1. 宏定义：启用实现，并设定内存基址为 0x80000000
#define MINIRV32IMA_IMPLEMENTATION
#define MINIRV32_CUSTOM_MEMORY_FUNCTIONS
#define MINIRV32_RAM_IMAGE_OFFSET 0x80000000

#include "mini-rv32ima.h"

// 2. 状态定义
static struct MiniRV32IMAState core;
static uint8_t *ram;
static uint32_t ram_size_bytes;

// 3. 初始化：分配内存并重置
void ref_init(uint32_t mem_size, uint32_t start_pc) {
    if (ram) free(ram);
    ram = (uint8_t*)malloc(mem_size);
    ram_size_bytes = mem_size;
    memset(ram, 0, mem_size);
    
    memset(&core, 0, sizeof(core));
    core.pc = start_pc;
}

// 4. 加载二进制：必须处理 offset - base
void ref_load_bin(uint32_t addr, uint8_t *data, uint32_t len) {
    if (addr < MINIRV32_RAM_IMAGE_OFFSET) return; // 越界保护
    uint32_t physical_offset = addr - MINIRV32_RAM_IMAGE_OFFSET;
    
    if (physical_offset + len <= ram_size_bytes) {
        memcpy(ram + physical_offset, data, len);
    }
}

// 5. 核心：跑一步
void ref_step() {
    // 参数：core, ram, instruction(0=fetch), elapsed_us(1), count(1)
    MiniRV32IMAStep(&core, ram, 0, 1, 1);
}

// 6. 状态读取接口
uint32_t ref_get_reg(int idx) {
    if (idx < 0 || idx > 31) return 0;
    return core.regs[idx];
}

uint32_t ref_get_pc() {
    return core.pc;
}

// 7. 必要的内存读写桩函数 (因为定义了 CUSTOM_MEMORY_FUNCTIONS)
uint32_t MiniRV32IMALoad(uint32_t ofs, uint32_t size) {
    if (ofs < MINIRV32_RAM_IMAGE_OFFSET) return 0;
    uint32_t p = ofs - MINIRV32_RAM_IMAGE_OFFSET;
    if (p + size > ram_size_bytes) return 0;
    
    uint32_t ret = 0;
    memcpy(&ret, ram + p, size);
    return ret;
}

void MiniRV32IMAStore(uint32_t ofs, uint32_t val, uint32_t size) {
    if (ofs < MINIRV32_RAM_IMAGE_OFFSET) return;
    uint32_t p = ofs - MINIRV32_RAM_IMAGE_OFFSET;
    if (p + size > ram_size_bytes) return;
    
    memcpy(ram + p, &val, size);
}

// 系统调用和 CSR 处理桩 (暂时留空，让它默认处理)
uint32_t MiniRV32IMAHandleException(uint32_t irq, uint32_t retval) {
    return retval;
}
uint32_t MiniRV32IMACSRRead(uint32_t csr) { return 0; }
uint32_t MiniRV32IMACSRWrite(uint32_t csr, uint32_t val) { return 0; }
