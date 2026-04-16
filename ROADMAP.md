# REMUR — 开发路线图与实现指南

> 本文档描述了从零到启动 Linux 的完整开发路径，每个里程碑都有具体的实现思路。

---

## 总览：开发里程碑

```
M1: RV32I 骨架       ──→  ✅ 能跑简单裸机程序 + riscv-tests 37/37
M2: 扩展指令集       ──→  ✅ RV32M + RV32A 全部通过 (55/55)
M3: 特权架构         ──→  ✅ M/S/U 模式 + 异常委托 + Sv32 页表 (77/77)
M4: 性能基准与优化   ──→  ✅ criterion + 译码缓存 + u8 寄存器索引
M5: SoC 外设         ──→  ✅ Device trait + CLINT/PLIC/UART + ELF loader + arch-test 适配 (84/84)
M6: 启动 Linux       ──→  SBI + DTB + 内核加载 → Linux shell
M7: 调试/测试        ──→  贯穿 M1-M6，itrace/difftest
M8: 高级优化（可选） ──→  基本块缓存 + JIT
```

---

## M1：RV32I 解释器骨架

### 目标
- 一个能执行 RV32I 指令的最简模拟器
- 能加载二进制文件并执行，类似你现在的 minirvEMU.c

### 步骤

#### Step 1.1：创建 Rust 项目
```bash
cd YSYX_Practice/REMUR
cargo init --name remur
```

项目结构：
```
REMUR/
├── Cargo.toml
├── FEATURES.md
├── ROADMAP.md
└── src/
    ├── main.rs          # CLI 入口
    ├── cpu.rs           # Hart 结构体 + 主循环
    ├── decode.rs        # 指令译码
    ├── execute.rs       # 指令执行
    └── memory.rs        # 物理内存
```

#### Step 1.2：定义核心数据结构

```rust
// cpu.rs
pub struct Hart {
    pub regs: [u32; 32],    // x0-x31
    pub pc: u32,            // 程序计数器
    // 后续扩展：csr, privilege_mode, ...
}

impl Hart {
    pub fn new() -> Self {
        Hart {
            regs: [0; 32],
            pc: 0,
        }
    }

    /// 读寄存器（x0 永远返回 0）
    pub fn read_reg(&self, idx: usize) -> u32 {
        if idx == 0 { 0 } else { self.regs[idx] }
    }

    /// 写寄存器（x0 写入被忽略）
    pub fn write_reg(&mut self, idx: usize, val: u32) {
        if idx != 0 {
            self.regs[idx] = val;
        }
    }
}
```

```rust
// memory.rs
pub struct Memory {
    data: Vec<u8>,  // 字节寻址
}

impl Memory {
    pub fn new(size: usize) -> Self {
        Memory { data: vec![0; size] }
    }

    pub fn read8(&self, addr: u32) -> u8 { ... }
    pub fn read16(&self, addr: u32) -> u16 { ... }  // 小端
    pub fn read32(&self, addr: u32) -> u32 { ... }  // 小端
    pub fn write8(&mut self, addr: u32, val: u8) { ... }
    pub fn write16(&mut self, addr: u32, val: u16) { ... }
    pub fn write32(&mut self, addr: u32, val: u32) { ... }
    pub fn load_binary(&mut self, offset: u32, data: &[u8]) { ... }
}
```

#### Step 1.3：指令译码

所有 RV32 指令都是 32 位，低 7 位是 opcode。根据 opcode 确定指令格式，然后提取字段。

```rust
// decode.rs — 指令格式字段提取

/// 从 32 位指令中提取各字段
pub fn opcode(inst: u32) -> u32 { inst & 0x7F }
pub fn rd(inst: u32) -> usize { ((inst >> 7) & 0x1F) as usize }
pub fn funct3(inst: u32) -> u32 { (inst >> 12) & 0x7 }
pub fn rs1(inst: u32) -> usize { ((inst >> 15) & 0x1F) as usize }
pub fn rs2(inst: u32) -> usize { ((inst >> 20) & 0x1F) as usize }
pub fn funct7(inst: u32) -> u32 { (inst >> 25) & 0x7F }

/// I-type 立即数（符号扩展）
pub fn imm_i(inst: u32) -> i32 { (inst as i32) >> 20 }

/// S-type 立即数
pub fn imm_s(inst: u32) -> i32 {
    let lo = (inst >> 7) & 0x1F;
    let hi = (inst >> 25) & 0x7F;
    (((hi << 5) | lo) as i32) << 20 >> 20  // 符号扩展 12 位
}

/// B-type 立即数
pub fn imm_b(inst: u32) -> i32 {
    let b11  = (inst >> 7) & 0x1;
    let b4_1 = (inst >> 8) & 0xF;
    let b10_5= (inst >> 25) & 0x3F;
    let b12  = (inst >> 31) & 0x1;
    let imm = (b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1);
    (imm as i32) << 19 >> 19  // 符号扩展 13 位
}

/// U-type 立即数（已左移 12 位）
pub fn imm_u(inst: u32) -> u32 { inst & 0xFFFFF000 }

/// J-type 立即数
pub fn imm_j(inst: u32) -> i32 {
    let b19_12 = (inst >> 12) & 0xFF;
    let b11    = (inst >> 20) & 0x1;
    let b10_1  = (inst >> 21) & 0x3FF;
    let b20    = (inst >> 31) & 0x1;
    let imm = (b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1);
    (imm as i32) << 11 >> 11  // 符号扩展 21 位
}
```

#### Step 1.4：取指–译码–执行主循环

```rust
// cpu.rs 中的主循环
impl Hart {
    pub fn step(&mut self, mem: &mut Memory) {
        let inst = mem.read32(self.pc);
        let op = decode::opcode(inst);
        match op {
            0b0110011 => self.exec_r_type(inst, mem),    // R-type (ADD/SUB/...)
            0b0010011 => self.exec_i_type(inst, mem),    // I-type ALU (ADDI/...)
            0b0000011 => self.exec_load(inst, mem),      // Load (LB/LH/LW/...)
            0b0100011 => self.exec_store(inst, mem),     // Store (SB/SH/SW)
            0b1100011 => self.exec_branch(inst, mem),    // Branch (BEQ/BNE/...)
            0b0110111 => self.exec_lui(inst),            // LUI
            0b0010111 => self.exec_auipc(inst),          // AUIPC
            0b1101111 => self.exec_jal(inst),            // JAL
            0b1100111 => self.exec_jalr(inst, mem),      // JALR
            0b1110011 => self.exec_system(inst, mem),    // ECALL/EBREAK/CSR
            0b0001111 => { self.pc += 4; }               // FENCE (NOP)
            _ => panic!("Unknown opcode: 0b{:07b} at PC=0x{:08x}", op, self.pc),
        }
    }

    pub fn run(&mut self, mem: &mut Memory, max_cycles: u64) {
        for _ in 0..max_cycles {
            self.step(mem);
        }
    }
}
```

#### Step 1.5：逐类实现执行函数

以 R-type 为例：
```rust
fn exec_r_type(&mut self, inst: u32, _mem: &mut Memory) {
    let rd = decode::rd(inst);
    let rs1_val = self.read_reg(decode::rs1(inst));
    let rs2_val = self.read_reg(decode::rs2(inst));
    let f3 = decode::funct3(inst);
    let f7 = decode::funct7(inst);

    let result = match (f3, f7) {
        (0x0, 0x00) => rs1_val.wrapping_add(rs2_val),       // ADD
        (0x0, 0x20) => rs1_val.wrapping_sub(rs2_val),       // SUB
        (0x1, 0x00) => rs1_val << (rs2_val & 0x1F),         // SLL
        (0x2, 0x00) => ((rs1_val as i32) < (rs2_val as i32)) as u32, // SLT
        (0x3, 0x00) => (rs1_val < rs2_val) as u32,          // SLTU
        (0x4, 0x00) => rs1_val ^ rs2_val,                   // XOR
        (0x5, 0x00) => rs1_val >> (rs2_val & 0x1F),         // SRL
        (0x5, 0x20) => ((rs1_val as i32) >> (rs2_val & 0x1F)) as u32, // SRA
        (0x6, 0x00) => rs1_val | rs2_val,                   // OR
        (0x7, 0x00) => rs1_val & rs2_val,                   // AND
        _ => panic!("Unknown R-type: f3={}, f7={}", f3, f7),
    };

    self.write_reg(rd, result);
    self.pc = self.pc.wrapping_add(4);
}
```

#### Step 1.6：main.rs 入口

```rust
use std::env;
use std::fs;

mod cpu;
mod decode;
mod memory;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: remur <binary_file>");
        std::process::exit(1);
    }

    let binary = fs::read(&args[1]).expect("Failed to read binary file");

    let mut mem = memory::Memory::new(128 * 1024 * 1024); // 128MB
    mem.load_binary(0x8000_0000, &binary);

    let mut hart = cpu::Hart::new();
    hart.pc = 0x8000_0000;

    hart.run(&mut mem, 100_000);

    println!("PC=0x{:08x}", hart.pc);
    println!("a0(x10)={}", hart.read_reg(10));
}
```

### 验证
- 手写一个简单的 RV32I 汇编程序（如 1+2+...+10），汇编成二进制
- 用 remur 加载执行，检查 a0 结果是否正确

### 验证工具链准备

运行 riscv-tests 需要 RISC-V 交叉编译工具链。项目通过 WSL 编译测试：

```powershell
# 1. 在 WSL 中安装交叉编译器（Ubuntu/Debian）
wsl sudo apt install gcc-riscv64-unknown-elf binutils-riscv64-unknown-elf

# 2. 编译所有 RV32 测试并转为 .bin（在项目根目录执行）
.\scripts\setup_tests.ps1

# 3. 编译后的二进制文件在 tests\bins\ 下
#    例如 rv32ui-p-add.bin, rv32ui-p-addi.bin, ...

# 4. 运行测试（实现 cargo test 后）
cargo test
```

手动编译单个汇编程序：
```bash
# 在 WSL 中
riscv64-unknown-elf-as -march=rv32i -mabi=ilp32 -o test.o test.S
riscv64-unknown-elf-ld -T link.ld -m elf32lriscv -o test.elf test.o
riscv64-unknown-elf-objcopy -O binary test.elf test.bin
```

---

## M2：扩展指令集

### Step 2.1：RV32M
在 `exec_r_type` 的 match 中，当 `funct7 == 0x01` 时进入 M 扩展分支：
```rust
(f3, 0x01) => match f3 {
    0x0 => rs1_val.wrapping_mul(rs2_val),                    // MUL
    0x1 => (((rs1 as i64) * (rs2 as i64)) >> 32) as u32,    // MULH
    0x2 => (((rs1 as i32 as i64) * (rs2 as u64 as i64)) >> 32) as u32, // MULHSU
    0x3 => (((rs1 as u64) * (rs2 as u64)) >> 32) as u32,    // MULHU
    0x4 => if rs2 == 0 { u32::MAX } else { ((rs1 as i32).wrapping_div(rs2 as i32)) as u32 }, // DIV
    0x5 => if rs2 == 0 { u32::MAX } else { rs1.wrapping_div(rs2) }, // DIVU
    0x6 => if rs2 == 0 { rs1 } else { ((rs1 as i32).wrapping_rem(rs2 as i32)) as u32 }, // REM
    0x7 => if rs2 == 0 { rs1 } else { rs1.wrapping_rem(rs2) }, // REMU
}
```

### Step 2.2：RV32A
新增 opcode `0b0101111` (AMO) 处理。关键是 LR/SC 对需要维护一个 reservation set。

### Step 2.3：Zicsr
在 `exec_system` 中，当 funct3 != 0 时为 CSR 指令。需要新增 CSR 存储。

### 验证
- 运行 riscv-tests 的 rv32ui、rv32um、rv32ua 测试套件

---

## M3：特权架构

### 目标
- 实现 M/S/U 三级特权模式，全部异常/trap 处理，Sv32 虚拟内存
- 通过 riscv-tests rv32mi（16/16）+ rv32si（6/6）全部测试

### 已完成

#### 特权核心
- 三级特权模式（M/S/U），ecall 区分 cause
- MRET/SRET 特权恢复（含 MPRV 清除）
- S-mode CSR 别名（sstatus/sie/sip ↔ mstatus/mie/mip）
- 异常委托（medeleg/mideleg → trap 到 S-mode 或 M-mode）
- CSR 访问控制（特权级 + 只读位 + TVM/TSR/TW 检查）
- 非法指令异常（未知指令/特权不足）
- 地址未对齐异常（LH/LW/SH/SW/跳转）

#### 虚拟内存
- Sv32 两级页表遍历（4KB 页 + 4MB 超级页）
- PTE 权限检查（V/R/W/X/U/A/D + SUM/MXR）
- 超级页对齐检查（PPN[0] 非零 → 页错误）
- A/D 位缺失 → 页面错误（软件管理模式）
- MPRV 支持（M-mode Load/Store 使用 MPP 翻译）

#### 性能计数器
- minstret/mcycle 64 位计数器（支持溢出进位）
- 写 minstret/minstreth 后抑制本次递增（规范要求）
- cycle/instret 只读影子 CSR → 映射到 M-mode 计数器
- mcounteren/scounteren 访问控制

#### 其他
- PMP 基础 CSR 读写（pmpcfg0/pmpaddr0）
- Debug trigger CSR 桩（tselect/tdata1 报告无触发器）

### 验证
- 运行 riscv-tests 的 privilege 测试
- 自己写一个裸机程序测试 M→S 模式切换

---

## M4：性能基准与优化

### 目标
- 建立 criterion benchmark 基础设施，量化 MIPS 性能
- 通过 feature flag 实现可切换优化，支持 A/B 对比

### 已完成

#### Step 4.1：Benchmark 基础设施
- criterion 0.7 集成，Release profile 调优（LTO + codegen-units=1）
- 全量 77 测试聚合 benchmark + 单项测试 benchmark

#### Step 4.2：Feature Flag 框架
```toml
[features]
default = ["cached-decode", "software-tlb"]
cached-decode = []    # 512 条目直接映射译码缓存
software-tlb = []     # 预留 TLB 缓存
```

#### Step 4.3：译码缓存
- 512 条目直接映射缓存，`cached-decode` feature 控制
- 测试不回归，小规模测试性能中性

#### Step 4.4：Instruction u8 寄存器索引
- 寄存器索引 usize→u8，内存减小 62%

### 待完成
- [ ] 查表译码（opcode → funct3 → funct7）
- [ ] 软件 TLB（缓存 VA→PA 映射）
- [ ] 基本块缓存

---

## M5：SoC 外设

> **设计原则**：易扩展、易读、性能高。Device trait + 具体类型字段（静态分派，零开销）。

### 已完成

#### Step 5.1：Bus 重构 + Device trait
- `Device` trait：read8/16/32, write8/16/32 带默认实现
- `Bus` 持有具体类型字段（ram/uart/clint/plic），match 路由地址
- Memory 改为 offset-based（0-indexed），Bus 负责地址映射

#### Step 5.2：CLINT 实现 + CPU 集成
- msip/mtimecmp/mtime 完整 MMIO 读写
- `tick()` 每 CPU 周期推进 mtime
- `update_mip()` 同步 MTIP/MSIP/MEIP 到 MIP 寄存器
- `check_pending_interrupts()` 完整中断优先级 + 委托逻辑
- MIP CSR 写入保护硬件控制位

#### Step 5.3：PLIC 极简版
- 1024 源优先级 + 1 context enable/threshold/claim
- `has_pending_interrupt()` 驱动 MIP.MEIP

#### Step 5.4：UART 16550 极简版
- THR 写入 → print! 输出
- LSR 读取 → 0x60（发送空+完成）

#### Step 5.5：ELF 加载器
- goblin 0.9 解析 ELF（PT_LOAD 段 + tohost 符号）
- 自动检测 ELF vs raw .bin
- 77/77 ELF 测试全通过

#### Step 5.6：PLIC claim 修复 + Device trait 读副作用
- Device trait read 方法改为 `&mut self`（MMIO 读取常有副作用）
- PLIC claim 原子清除 pending + 设置 claimed
- PLIC complete 清除 claimed（而非 pending）

#### Step 5.7：裸机外设集成测试
- 7 个手写机器码测试（UART/CLINT/PLIC）
- 涵盖：写入输出、LSR 状态、mtime 递增、定时器中断、软件中断、PLIC claim/priority

#### Step 5.8：签名导出 + arch-test 框架适配
- ELF 加载器提取 `begin_signature`/`end_signature` 符号
- CLI `--signature <file>` 导出签名区域（每行 8 位 hex）
- CLI `--tohost <hex>` / `--cycles <n>` 参数
- `config/remur/remur-rv32ima/` — 完整 arch-test 配置
- `scripts/run-arch-test.sh` — 一键合规测试脚本
- `scripts/pre-push` — git push 前自动 cargo test

### 待完成
- [ ] UART RBR 接收 + IER 中断

### 验证
- 84/84 测试通过（77 riscv-tests + 7 外设集成测试）
- ELF loader 自动提取 tohost + 签名区域符号
- `--signature` 选项可导出 arch-test 格式签名
- arch-test 框架配置就绪（`config/remur/remur-rv32ima/`）
- pre-push hook 保障每次推送前测试通过

---

## M6：启动 Linux

### Step 6.1：内嵌 SBI

M-mode 固件处理 S-mode 的 `ecall`：
```rust
fn handle_sbi_call(&mut self, ...) {
    match (a7, a6) {  // EID, FID
        (0x01, _) => { /* console_putchar: 输出 a0 */ }
        (0x02, _) => { /* console_getchar: 读入到 a0 */ }
        (0x00, _) => { /* set_timer: 设置 mtimecmp */ }
        (0x08, _) => { /* shutdown */ }
        (0x54494D45, 0) => { /* Timer: set_timer */ }
        ...
    }
}
```

### Step 6.2：DTB
使用现成的 DTB 文件（从 QEMU 或手写 DTS 编译），描述：
- CPU 信息（rv32ima, mmu-type=sv32）
- 内存区域
- CLINT/PLIC/UART 地址
- bootargs（console=ttyS0）
- chosen 节点（initrd 地址）

### Step 6.3：启动序列

> **内核格式**：支持 **Linux Image**（flat binary）和 **ELF** 格式（M5 已实现 ELF 加载器）。

```
1. 加载 OpenSBI/内嵌SBI 到 0x8000_0000 (M-mode 入口)
2. 加载 Linux Image 到 0x8020_0000
3. 加载 DTB 到 0x8200_0000
4. 加载 initramfs 到 0x8300_0000
5. Hart 从 0x8000_0000 开始执行 (M-mode)
6. SBI 初始化 → 跳转到 0x8020_0000 (S-mode)
   a0 = 0 (hartid), a1 = 0x8200_0000 (DTB 地址)
7. Linux 内核启动
```

如何获取这些文件：
- **OpenSBI**: `make PLATFORM=generic CROSS_COMPILE=riscv64-unknown-elf- FW_JUMP_ADDR=0x80200000`
- **Linux Image**: 用 buildroot 交叉编译 32 位内核，取 `output/images/Image`
- **initramfs**: buildroot 生成的 `rootfs.cpio`
- **DTB**: 手写 `.dts` 后用 `dtc` 编译，或从 QEMU `dumpdtb` 导出再修改

### 验证
- 看到 Linux 内核打印启动信息
- 进入 busybox shell

---

## M7：调试工具（贯穿始终）

### itrace
```rust
if ITRACE_ENABLED {
    eprintln!("0x{:08x}: {:08x}  {}", pc, inst, disassemble(inst));
}
```

### difftest（可选）
与 Spike 逐指令对比寄存器状态，一旦不一致立即报错。

---

## M8：高级优化（可选）

### 基本块缓存
```rust
struct BasicBlock {
    pc: u32,
    instructions: Vec<DecodedInst>,  // 预译码指令序列
    next_pc: u32,                     // 块结束后的下一个 PC
}
```
遇到分支/跳转指令时结束当前块，下次执行同一 PC 时直接取缓存。

### JIT（Just-In-Time 编译）
将热点基本块翻译为宿主机原生指令（x86/ARM），跳过解释执行。
需要运行时代码生成框架（如 cranelift 或手写 assembler）。

---

## 关键对照：minirvEMU.c → Rust 映射

| C 代码 | Rust 对应 |
|--------|-----------|
| `uint32_t R[16]` | `Hart.regs: [u32; 32]`（扩展到 32 个） |
| `uint32_t M[MEM_SIZE]` | `Memory.data: Vec<u8>`（字节寻址更灵活） |
| `uint32_t PC` | `Hart.pc: u32` |
| 全局变量 | 封装在 struct 中（Hart, Memory, Bus） |
| `switch(opcode)` | `match opcode {}`（Rust 模式匹配） |
| `M[addr >> 2]` | `mem.read32(addr)`（内部处理字节偏移） |
| 位运算提取字段 | `decode.rs` 中的辅助函数 |
| `inst_cycle()` 返回 int | `Hart::step()` 返回 `Result<(), Exception>` |

---

## 推荐参考资料

1. **RISC-V 规范**
   - [Volume I: Unprivileged Spec](https://riscv.org/specifications/) — 指令集定义
   - [Volume II: Privileged Spec](https://riscv.org/specifications/privileged-isa/) — 特权架构
2. **现有实现参考**
   - [rvemu (Rust)](https://github.com/d0iasm/rvemu) — Rust RV64 模拟器
   - [riscv-rust (Rust)](https://github.com/takahirox/riscv-rust) — 可跑 Linux 的 RV64
   - [mini-rv32ima (C)](https://github.com/cnlohr/mini-rv32ima) — 极简 C 实现，可跑 Linux
3. **riscv-tests**
   - [官方测试套件](https://github.com/riscv-software-src/riscv-tests)
4. **SBI 规范**
   - [RISC-V SBI Specification](https://github.com/riscv-non-isa/riscv-sbi-doc)
