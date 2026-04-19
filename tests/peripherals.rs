//! 裸机外设集成测试
//!
//! 用手工编码的 RV32I 机器码验证 CLINT/UART/PLIC 功能。
//! 不依赖交叉编译器。

use remur::bus::{Bus, RAM_BASE};
use remur::cpu::Hart;
use remur::memory::Memory;

const MEM_SIZE: usize = 4 * 1024 * 1024; // 4MB 足够测试
const TOHOST: u32 = 0x8000_1000;

/// 辅助：把机器码加载到 RAM 并配置 Hart
fn setup(code: &[u32]) -> (Hart, Bus) {
    let mem = Memory::new(MEM_SIZE);
    let mut bus = Bus::new(mem);
    bus.tohost_addr = Some(TOHOST);

    for (i, &inst) in code.iter().enumerate() {
        bus.ram.write32((i * 4) as u32, inst);
    }

    let mut hart = Hart::new();
    hart.pc = RAM_BASE;
    (hart, bus)
}

// ── RISC-V 机器码编码辅助 ────────────────────────────────────
fn lui(rd: u32, imm20: u32) -> u32 {
    (imm20 << 12) | (rd << 7) | 0x37
}

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xFFF;
    (imm << 20) | (rs1 << 15) | (0 << 12) | (rd << 7) | 0x13
}

#[allow(dead_code)]
fn ori(rd: u32, rs1: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xFFF;
    (imm << 20) | (rs1 << 15) | (6 << 12) | (rd << 7) | 0x13
}

fn sw(rs2: u32, base: u32, offset: i32) -> u32 {
    let imm = (offset as u32) & 0xFFF;
    let imm_11_5 = (imm >> 5) & 0x7F;
    let imm_4_0 = imm & 0x1F;
    (imm_11_5 << 25) | (rs2 << 20) | (base << 15) | (2 << 12) | (imm_4_0 << 7) | 0x23
}

fn sb(rs2: u32, base: u32, offset: i32) -> u32 {
    let imm = (offset as u32) & 0xFFF;
    let imm_11_5 = (imm >> 5) & 0x7F;
    let imm_4_0 = imm & 0x1F;
    (imm_11_5 << 25) | (rs2 << 20) | (base << 15) | (0 << 12) | (imm_4_0 << 7) | 0x23
}

fn lw(rd: u32, base: u32, offset: i32) -> u32 {
    let imm = (offset as u32) & 0xFFF;
    (imm << 20) | (base << 15) | (2 << 12) | (rd << 7) | 0x03
}

fn csrrs(rd: u32, csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (2 << 12) | (rd << 7) | 0x73
}

fn csrrw(rd: u32, csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (1 << 12) | (rd << 7) | 0x73
}

fn csrrsi(rd: u32, csr: u32, imm5: u32) -> u32 {
    (csr << 20) | (imm5 << 15) | (6 << 12) | (rd << 7) | 0x73
}

fn jal(rd: u32, offset: i32) -> u32 {
    let imm = (offset as u32) & 0x1FFFFF;
    let bit20 = (imm >> 20) & 1;
    let bits10_1 = (imm >> 1) & 0x3FF;
    let bit11 = (imm >> 11) & 1;
    let bits19_12 = (imm >> 12) & 0xFF;
    (bit20 << 31) | (bits10_1 << 21) | (bit11 << 20) | (bits19_12 << 12) | (rd << 7) | 0x6F
}

fn bne(rs1: u32, rs2: u32, offset: i32) -> u32 {
    let imm = (offset as u32) & 0x1FFF;
    let bit12 = (imm >> 12) & 1;
    let bits10_5 = (imm >> 5) & 0x3F;
    let bits4_1 = (imm >> 1) & 0xF;
    let bit11 = (imm >> 11) & 1;
    (bit12 << 31)
        | (bits10_5 << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (1 << 12)
        | (bits4_1 << 8)
        | (bit11 << 7)
        | 0x63
}

fn beq(rs1: u32, rs2: u32, offset: i32) -> u32 {
    let imm = (offset as u32) & 0x1FFF;
    let bit12 = (imm >> 12) & 1;
    let bits10_5 = (imm >> 5) & 0x3F;
    let bits4_1 = (imm >> 1) & 0xF;
    let bit11 = (imm >> 11) & 1;
    (bit12 << 31)
        | (bits10_5 << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (0 << 12)
        | (bits4_1 << 8)
        | (bit11 << 7)
        | 0x63
}

#[allow(dead_code)]
fn mret() -> u32 {
    0x30200073
}

fn nop() -> u32 {
    addi(0, 0, 0)
}

#[allow(dead_code)]
fn add(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (rs2 << 20) | (rs1 << 15) | (0 << 12) | (rd << 7) | 0x33
}

// ── 测试用例 ────────────────────────────────────────────────

/// 测试 UART 输出：写字符到 UART THR 寄存器
#[test]
fn uart_write_hello() {
    // 程序：写 "Hi" 到 UART，然后写 1 到 tohost
    // t0 = UART_BASE = 0x1000_0000
    // t1 = tohost = 0x8000_1000
    let code = vec![
        lui(5, 0x10000),         // t0 = 0x1000_0000 (UART base)
        addi(6, 0, 'H' as i32),  // t1 = 'H'
        sb(6, 5, 0),             // UART[0] = 'H'
        addi(6, 0, 'i' as i32),  // t1 = 'i'
        sb(6, 5, 0),             // UART[0] = 'i'
        addi(6, 0, '\n' as i32), // t1 = '\n'
        sb(6, 5, 0),             // UART[0] = '\n'
        // 写 1 到 tohost 表示 PASS
        lui(7, 0x80001), // t2 = 0x8000_1000
        addi(8, 0, 1),   // t3 = 1
        sw(8, 7, 0),     // tohost = 1
        nop(),           // 多余指令以防万一
    ];

    let (mut hart, mut bus) = setup(&code);
    let result = hart.run(&mut bus, 100);
    assert_eq!(result, Some(1), "UART test should PASS (tohost=1)");
}

/// 测试 UART LSR 读取：总是返回 0x60
#[test]
fn uart_lsr_always_ready() {
    // 读 UART LSR (0x1000_0005) 到 t1，验证 == 0x60
    // 然后写 1 到 tohost
    let code = vec![
        lui(5, 0x10000), // t0 = 0x1000_0000
        // 用 lw 读 offset 4 的字（包含 LSR byte at offset 5）
        // 不过 read8 更准确，用 lb 来读 offset 5
        // LBU: funct3=4
        0x00528303,       // lbu t1, 5(t0) — 手工编码 lbu x6, 5(x5)
        addi(7, 0, 0x60), // t2 = 0x60
        bne(6, 7, 16),    // if t1 != 0x60, skip to FAIL
        // PASS
        lui(8, 0x80001), // t3 = 0x8000_1000
        addi(9, 0, 1),
        sw(9, 8, 0), // tohost = 1
        jal(0, 8),   // jump over FAIL
        // FAIL
        lui(8, 0x80001),
        addi(9, 0, 3), // tohost = 3 (FAIL case 1)
        sw(9, 8, 0),
        nop(),
    ];

    let (mut hart, mut bus) = setup(&code);
    let result = hart.run(&mut bus, 100);
    assert_eq!(result, Some(1), "UART LSR should be 0x60");
}

/// 测试 CLINT mtime 递增
#[test]
fn clint_mtime_increments() {
    // 读 CLINT mtime (0x0200_BFF8) 两次，第二次应该 > 第一次
    // CLINT mtime low = 0x0200_BFF8
    // lui t0, 0x0200C -> t0 = 0x0200_C000, then lw t1, -8(t0)
    let code = vec![
        lui(5, 0x0200C), // t0 = 0x0200_C000
        lw(6, 5, -8),    // t1 = mtime_low (first read)
        nop(),
        nop(),
        nop(), // burn some cycles
        nop(),
        nop(),
        nop(),
        lw(7, 5, -8), // t2 = mtime_low (second read)
        // t2 should be > t1 (mtime increments each cycle)
        beq(6, 7, 16), // if t1 == t2, jump to FAIL (should have incremented)
        // PASS
        lui(8, 0x80001),
        addi(9, 0, 1),
        sw(9, 8, 0), // tohost = 1
        jal(0, 8),   // skip FAIL
        // FAIL
        lui(8, 0x80001),
        addi(9, 0, 3),
        sw(9, 8, 0),
        nop(),
    ];

    let (mut hart, mut bus) = setup(&code);
    let result = hart.run(&mut bus, 200);
    assert_eq!(
        result,
        Some(1),
        "CLINT mtime should increment between reads"
    );
}

/// 测试 CLINT 定时器中断
#[test]
fn clint_timer_interrupt() {
    // 程序流程：
    // 1. 设置 mtvec 指向中断处理函数
    // 2. 读 mtime，设 mtimecmp = mtime + 50
    // 3. 使能 MIE + MTIE
    // 4. 循环等待（handler 会写 tohost=1）
    //
    // 寄存器分配：
    //   t0(x5) = CLINT base, t1(x6) = temp, t2(x7) = tohost base
    //   t3(x28) = temp

    let handler_offset = 22 * 4; // handler at instruction 22

    let code = vec![
        // 0: 设置 mtvec = RAM_BASE + handler_offset (Direct mode)
        lui(6, 0x80000),                   // t1 = 0x8000_0000
        addi(6, 6, handler_offset as i32), // t1 = handler address
        csrrw(0, 0x305, 6),                // mtvec = t1
        // 3: 读 CLINT mtime_low
        lui(5, 0x0200C), // t0 = 0x0200_C000
        lw(6, 5, -8),    // t1 = mtime_low
        // 5: 设 mtimecmp = mtime + 50
        addi(6, 6, 50), // t1 = mtime + 50
        // CLINT mtimecmp_low = 0x0200_4000
        // lui t0, 0x02004 -> t0 = 0x0200_4000
        lui(5, 0x02004), // t0 = 0x0200_4000
        sw(6, 5, 0),     // mtimecmp_low = mtime + 50
        // mtimecmp_high = 0
        sw(0, 5, 4), // mtimecmp_high = 0
        // 9: 使能 mstatus.MIE (bit 3)
        csrrsi(0, 0x300, 8), // mstatus |= MIE (imm=8, bit3)
        // 10: 使能 mie.MTIE (bit 7)
        addi(6, 0, 0x80),   // t1 = 0x80 (MTIE bit)
        csrrs(0, 0x304, 6), // mie |= MTIE
        // 12: 等待循环 — 被中断打断后 handler 写 tohost
        nop(),       // 12
        nop(),       // 13
        nop(),       // 14
        nop(),       // 15
        jal(0, -16), // 16: loop back to 12 (offset = -4*4 = -16)
        // 17: 超时 FAIL（不应到达这里）
        lui(7, 0x80001), // t2 = 0x8000_1000
        addi(6, 0, 3),
        sw(6, 7, 0), // tohost = 3 (FAIL)
        nop(),
        nop(),
        // 22: 中断处理函数 (handler_offset = 22*4 = 88 = 0x58)
        // mcause 应为 0x8000_0007 (interrupt + MTI)
        // 写 tohost = 1 (PASS)
        lui(7, 0x80001), // t2 = 0x8000_1000
        addi(6, 0, 1),
        sw(6, 7, 0), // tohost = 1 (PASS!)
        nop(),
    ];

    let (mut hart, mut bus) = setup(&code);
    let result = hart.run(&mut bus, 500);
    assert_eq!(
        result,
        Some(1),
        "Timer interrupt should fire and write tohost=1"
    );

    // 验证 mcause = 0x8000_0007 (interrupt bit + cause 7 = MTI)
    let mcause = hart.csrs[0x342];
    assert_eq!(
        mcause, 0x8000_0007,
        "mcause should be 0x8000_0007 (MTI interrupt), got 0x{:08x}",
        mcause
    );
}

/// 测试 CLINT 软件中断 (MSIP)
#[test]
fn clint_software_interrupt() {
    let handler_offset = 16 * 4; // handler at instruction 16

    let code = vec![
        // 0: 设置 mtvec
        lui(6, 0x80000),
        addi(6, 6, handler_offset as i32),
        csrrw(0, 0x305, 6), // mtvec = handler
        // 3: 使能 mstatus.MIE
        csrrsi(0, 0x300, 8), // mstatus |= MIE
        // 4: 使能 mie.MSIE (bit 3)
        addi(6, 0, 8),      // t1 = 8 (MSIE bit)
        csrrs(0, 0x304, 6), // mie |= MSIE
        // 6: 写 CLINT msip = 1
        lui(5, 0x02000), // t0 = 0x0200_0000
        addi(6, 0, 1),
        sw(6, 5, 0), // CLINT msip = 1
        // 9: 等待中断
        nop(),       // 9
        nop(),       // 10
        nop(),       // 11
        jal(0, -12), // 12: loop back to 9
        // 13: FAIL
        lui(7, 0x80001),
        addi(6, 0, 3),
        sw(6, 7, 0),
        // 16: handler
        lui(7, 0x80001),
        addi(6, 0, 1),
        sw(6, 7, 0), // tohost = 1 (PASS)
        nop(),
    ];

    let (mut hart, mut bus) = setup(&code);
    let result = hart.run(&mut bus, 200);
    assert_eq!(result, Some(1), "Software interrupt should fire");

    let mcause = hart.csrs[0x342];
    assert_eq!(
        mcause, 0x8000_0003,
        "mcause should be 0x8000_0003 (MSI interrupt), got 0x{:08x}",
        mcause
    );
}

/// 测试 PLIC set_pending + claim (通过 Bus MMIO)
#[test]
fn plic_pending_and_claim() {
    let mem = Memory::new(MEM_SIZE);
    let mut bus = Bus::new(mem);

    // PLIC 地址 (absolute): base = 0x0C00_0000
    let plic = 0x0C00_0000u32;

    // 设置 IRQ 10 优先级 = 5: priority[10] at offset 0x28
    bus.write32(plic + 0x28, 5);
    // 使能 IRQ 10: enable at offset 0x2000, bit 10
    bus.write32(plic + 0x2000, 1 << 10);
    // 阈值 = 0: threshold at offset 0x200000
    bus.write32(plic + 0x200000, 0);

    // 触发 IRQ 10
    bus.plic.set_pending(10);

    // claim 应返回 10 并清除 pending
    let claimed = bus.read32(plic + 0x200004);
    assert_eq!(claimed, 10, "claim should return IRQ 10");

    // pending 应已清除
    assert!(
        !bus.plic.has_pending_m_interrupt(),
        "pending should be cleared after claim"
    );

    // complete
    bus.write32(plic + 0x200004, 10);

    // 再次 claim 应返回 0
    let claimed = bus.read32(plic + 0x200004);
    assert_eq!(claimed, 0, "no more pending interrupts");
}

/// 测试 PLIC 优先级选择：多个 pending 时返回最高优先级
#[test]
fn plic_priority_selection() {
    let mem = Memory::new(MEM_SIZE);
    let mut bus = Bus::new(mem);

    let plic = 0x0C00_0000u32;

    // 设置优先级
    bus.write32(plic + 5 * 4, 3); // IRQ 5, priority 3
    bus.write32(plic + 10 * 4, 7); // IRQ 10, priority 7 (highest)
    bus.write32(plic + 15 * 4, 1); // IRQ 15, priority 1
    // 使能 IRQ 5, 10, 15
    bus.write32(plic + 0x2000, (1 << 5) | (1 << 10) | (1 << 15));
    // 阈值 = 0
    bus.write32(plic + 0x200000, 0);

    // 触发所有三个中断
    bus.plic.set_pending(5);
    bus.plic.set_pending(10);
    bus.plic.set_pending(15);

    // claim 应返回 IRQ 10（最高优先级 7）
    let claimed = bus.read32(plic + 0x200004);
    assert_eq!(claimed, 10, "should claim highest priority IRQ");

    // 下次 claim 应返回 IRQ 5（优先级 3）
    let claimed = bus.read32(plic + 0x200004);
    assert_eq!(claimed, 5, "should claim next highest priority IRQ");

    // 下次 claim 应返回 IRQ 15（优先级 1）
    let claimed = bus.read32(plic + 0x200004);
    assert_eq!(claimed, 15, "should claim last remaining IRQ");

    // 无更多 pending
    let claimed = bus.read32(plic + 0x200004);
    assert_eq!(claimed, 0, "no more pending");
}

/// 测试 PLIC S-mode context (context1) claim 路径
#[test]
fn plic_s_mode_context_claim() {
    let mem = Memory::new(MEM_SIZE);
    let mut bus = Bus::new(mem);
    let plic = 0x0C00_0000u32;

    // IRQ10 priority=5
    bus.write32(plic + 0x28, 5);

    // context1 enable/threshold/claim:
    // enable:    0x002080 (ctx1, word0)
    // threshold: 0x201000
    // claim:     0x201004
    bus.write32(plic + 0x2080, 1 << 10);
    bus.write32(plic + 0x201000, 0);

    bus.plic.set_pending(10);
    assert!(
        bus.plic.has_pending_s_interrupt(),
        "S-mode context should observe pending IRQ10"
    );

    let claimed = bus.read32(plic + 0x201004);
    assert_eq!(claimed, 10, "S-mode context should claim IRQ10");
    assert!(
        !bus.plic.has_pending_s_interrupt(),
        "S-mode pending should clear after claim"
    );
}
