# REMUR — RV32IMA 指令集模拟器 功能清单

> **目标**：用 Rust 实现一个 RV32IMAZicsr_Zifencei 指令集模拟器，最终能启动 Linux。
> **架构预留**：多核（Hart）扩展、外设 trait 化。

---

## 一、CPU 核心模块

### 1.1 寄存器组
- [x] 32 个 32 位通用寄存器 `x0`–`x31`（`x0` 硬连线为 0）
- [x] 32 位程序计数器 `PC`
- [x] CSR 寄存器存储（哈希表或数组，按地址索引）

### 1.2 RV32I 基础指令集（47 条）

#### 算术指令
| 指令 | 格式 | 说明 |
|------|------|------|
| ADD  | R | rd = rs1 + rs2 |
| SUB  | R | rd = rs1 - rs2 |
| ADDI | I | rd = rs1 + imm |
| SLT  | R | rd = (rs1 < rs2) ? 1 : 0（有符号） |
| SLTI | I | rd = (rs1 < imm) ? 1 : 0（有符号） |
| SLTU | R | rd = (rs1 < rs2) ? 1 : 0（无符号） |
| SLTIU| I | rd = (rs1 < imm) ? 1 : 0（无符号） |

#### 逻辑指令
| 指令 | 格式 | 说明 |
|------|------|------|
| AND  | R | rd = rs1 & rs2 |
| OR   | R | rd = rs1 \| rs2 |
| XOR  | R | rd = rs1 ^ rs2 |
| ANDI | I | rd = rs1 & imm |
| ORI  | I | rd = rs1 \| imm |
| XORI | I | rd = rs1 ^ imm |

#### 移位指令
| 指令 | 格式 | 说明 |
|------|------|------|
| SLL  | R | rd = rs1 << rs2[4:0] |
| SRL  | R | rd = rs1 >> rs2[4:0]（逻辑） |
| SRA  | R | rd = rs1 >> rs2[4:0]（算术） |
| SLLI | I | rd = rs1 << shamt |
| SRLI | I | rd = rs1 >> shamt（逻辑） |
| SRAI | I | rd = rs1 >> shamt（算术） |

#### 高位立即数
| 指令 | 格式 | 说明 |
|------|------|------|
| LUI  | U | rd = imm << 12 |
| AUIPC| U | rd = PC + (imm << 12) |

#### 分支指令
| 指令 | 格式 | 说明 |
|------|------|------|
| BEQ  | B | if (rs1 == rs2) PC += imm |
| BNE  | B | if (rs1 != rs2) PC += imm |
| BLT  | B | if (rs1 < rs2) PC += imm（有符号） |
| BGE  | B | if (rs1 >= rs2) PC += imm（有符号） |
| BLTU | B | if (rs1 < rs2) PC += imm（无符号） |
| BGEU | B | if (rs1 >= rs2) PC += imm（无符号） |

#### 跳转指令
| 指令 | 格式 | 说明 |
|------|------|------|
| JAL  | J | rd = PC+4; PC += imm |
| JALR | I | rd = PC+4; PC = (rs1+imm) & ~1 |

#### 访存指令
| 指令 | 格式 | 说明 |
|------|------|------|
| LB   | I | rd = sign_ext(mem[rs1+imm][7:0]) |
| LH   | I | rd = sign_ext(mem[rs1+imm][15:0]) |
| LW   | I | rd = mem[rs1+imm][31:0] |
| LBU  | I | rd = zero_ext(mem[rs1+imm][7:0]) |
| LHU  | I | rd = zero_ext(mem[rs1+imm][15:0]) |
| SB   | S | mem[rs1+imm][7:0] = rs2[7:0] |
| SH   | S | mem[rs1+imm][15:0] = rs2[15:0] |
| SW   | S | mem[rs1+imm][31:0] = rs2 |

#### 系统指令
| 指令 | 格式 | 说明 |
|------|------|------|
| ECALL | I | 环境调用（系统调用/SBI 调用） |
| EBREAK| I | 断点异常 |
| FENCE | I | 内存屏障（解释器中可 NOP） |

### 1.3 RV32M 扩展（乘除法，8 条）
| 指令 | 说明 |
|------|------|
| MUL     | rd = (rs1 * rs2)[31:0] |
| MULH    | rd = (rs1 * rs2)[63:32]（有符号×有符号） |
| MULHSU  | rd = (rs1 * rs2)[63:32]（有符号×无符号） |
| MULHU   | rd = (rs1 * rs2)[63:32]（无符号×无符号） |
| DIV     | rd = rs1 / rs2（有符号） |
| DIVU    | rd = rs1 / rs2（无符号） |
| REM     | rd = rs1 % rs2（有符号） |
| REMU    | rd = rs1 % rs2（无符号） |

### 1.4 RV32A 扩展（原子操作，11 条）
| 指令 | 说明 |
|------|------|
| LR.W      | 带保留加载 |
| SC.W      | 条件存储 |
| AMOSWAP.W | 原子交换 |
| AMOADD.W  | 原子加法 |
| AMOAND.W  | 原子与 |
| AMOOR.W   | 原子或 |
| AMOXOR.W  | 原子异或 |
| AMOMAX.W  | 原子最大值（有符号） |
| AMOMIN.W  | 原子最小值（有符号） |
| AMOMAXU.W | 原子最大值（无符号） |
| AMOMINU.W | 原子最小值（无符号） |

### 1.5 Zicsr 扩展（CSR 操作，6 条）
| 指令 | 说明 |
|------|------|
| CSRRW  | 读旧值到 rd，写 rs1 到 CSR |
| CSRRS  | 读旧值到 rd，置位 CSR 中 rs1 对应位 |
| CSRRC  | 读旧值到 rd，清除 CSR 中 rs1 对应位 |
| CSRRWI | 同 CSRRW，但源为 5 位立即数 |
| CSRRSI | 同 CSRRS，但源为 5 位立即数 |
| CSRRCI | 同 CSRRC，但源为 5 位立即数 |

### 1.6 Zifencei 扩展
| 指令 | 说明 |
|------|------|
| FENCE.I | 指令缓存同步（解释器中可 NOP，有基本块缓存时需刷新） |

---

## 二、特权架构

### 2.1 三级特权模式
- [x] M-mode（机器态，最高特权，复位入口）
- [x] S-mode（监管态，Linux 内核运行于此）
- [x] U-mode（用户态，应用程序运行于此）
- [x] MRET/SRET 指令（从 trap 返回，切换特权级）
- [x] WFI 指令（等待中断）

### 2.2 M-mode CSR
| CSR | 地址 | 说明 |
|-----|------|------|
| mstatus   | 0x300 | 全局状态（MIE/MPIE/MPP/SPP 等） |
| misa      | 0x301 | ISA 描述 |
| medeleg   | 0x302 | 异常委托到 S-mode |
| mideleg   | 0x303 | 中断委托到 S-mode |
| mie       | 0x304 | 中断使能 |
| mtvec     | 0x305 | trap 入口地址 |
| mcounteren| 0x306 | 计数器访问许可 |
| mscratch  | 0x340 | M-mode 暂存寄存器 |
| mepc      | 0x341 | 异常返回地址 |
| mcause    | 0x342 | 异常/中断原因 |
| mtval     | 0x343 | 异常附加信息 |
| mip       | 0x344 | 挂起中断 |
| mhartid   | 0xF14 | Hart ID（只读） |

### 2.3 S-mode CSR
| CSR | 地址 | 说明 |
|-----|------|------|
| sstatus   | 0x100 | S-mode 状态（mstatus 的子集视图） |
| sie       | 0x104 | S-mode 中断使能 |
| stvec     | 0x105 | S-mode trap 入口 |
| scounteren| 0x106 | 计数器访问许可 |
| sscratch  | 0x140 | S-mode 暂存寄存器 |
| sepc      | 0x141 | S-mode 异常返回地址 |
| scause    | 0x142 | S-mode 异常原因 |
| stval     | 0x143 | S-mode 异常附加信息 |
| sip       | 0x144 | S-mode 挂起中断 |
| satp      | 0x180 | 页表基址 + 翻译模式 |

### 2.4 异常处理
- [x] 指令地址未对齐（Instruction address misaligned）
- [ ] 指令访问错误（Instruction access fault）
- [x] 非法指令（Illegal instruction）
- [x] 断点（Breakpoint）
- [x] 加载地址未对齐（Load address misaligned）
- [ ] 加载访问错误（Load access fault）
- [x] 存储地址未对齐（Store address misaligned）
- [ ] 存储访问错误（Store access fault）
- [x] 环境调用 U-mode（Environment call from U-mode）
- [x] 环境调用 S-mode（Environment call from S-mode）
- [x] 环境调用 M-mode（Environment call from M-mode）
- [x] 取指页面错误（Instruction page fault）
- [x] 加载页面错误（Load page fault）
- [x] 存储页面错误（Store page fault）

### 2.5 中断处理
- [x] M-mode 软件中断（MSI）— CLINT msip 驱动
- [x] M-mode 定时器中断（MTI）— CLINT mtime/mtimecmp 驱动
- [x] M-mode 外部中断（MEI）— PLIC pending 驱动
- [ ] S-mode 软件中断（SSI）
- [ ] S-mode 定时器中断（STI）
- [ ] S-mode 外部中断（SEI）
- [x] 中断委托机制（medeleg/mideleg）
- [x] 中断优先级判断（MEI > MSI > MTI > SEI > SSI > STI）
- [x] MIP 硬件位保护（MTIP/MSIP/MEIP 只由硬件控制，CSR 写入不可修改）

### 2.6 Sv32 虚拟内存
- [x] 两级页表遍历（VPN[1] → VPN[0] → PPN）
- [x] 4KB 页面大小
- [x] 4MB 超级页（Megapage）支持
- [x] PTE 权限位检查（V/R/W/X/U/A/D）
- [x] A/D 位检查 → 页面错误（软件管理模式，不自动设置）
- [x] 页面错误生成
- [ ] TLB 缓存（直接映射，加速翻译）
- [x] SFENCE.VMA 指令（当前无 TLB，为 NOP）

### 2.7 其他特权功能
- [x] MPRV 支持（M-mode Load/Store 使用 MPP 特权级翻译）
- [x] SUM 支持（S-mode 访问 U 页面）
- [x] MXR 支持（可执行页面可读）
- [x] TVM/TSR/TW 陷阱（虚拟化辅助位）
- [x] CSR 访问权限控制（特权级 + 只读位检查）
- [x] mcounteren/scounteren 计数器访问控制
- [x] minstret/mcycle 64 位性能计数器（含写抑制）
- [x] PMP 基础 CSR 读写（pmpcfg0/pmpaddr0）
- [x] Debug trigger CSR 桩（tselect/tdata1 报告无触发器）

---

## 三、SoC 外设

### 3.1 总线与地址空间
- [x] 统一 MMIO 分发框架（match 地址路由）
- [x] `Device` trait（read8/16/32, write8/16/32 带默认实现）
- [x] 具体类型字段 + 静态分派（零开销，非 Box<dyn>）

#### 默认地址映射
| 设备 | 起始地址 | 大小 |
|------|----------|------|
| CLINT   | 0x0200_0000 | 64KB |
| PLIC    | 0x0C00_0000 | 64MB |
| UART0   | 0x1000_0000 | 4KB |
| RAM     | 0x8000_0000 | 128MB+ |

### 3.2 CLINT（Core Local Interruptor）
- [x] mtime 寄存器（64 位实时计数器，每 CPU 周期递增）
- [x] mtimecmp 寄存器（定时器比较值，初始 u64::MAX）
- [x] 定时器中断产生（mtime >= mtimecmp 时置位 MIP.MTIP）
- [x] MSIP 寄存器（软件中断触发，置位 MIP.MSIP）
- [x] CLINT 集成到 CPU step()（每步 tick + update_mip）

### 3.3 PLIC（极简版）
- [x] 1024 源优先级数组
- [x] 1 context enable/threshold/claim
- [x] set_pending() / has_pending_interrupt() 接口
- [x] claim 读取原子清除 pending + 设置 claimed 位
- [x] complete 清除 claimed 位（正确语义）
- [x] 优先级选择（最高优先级 IRQ 优先 claim）

### 3.4 UART（极简版 16550）
- [x] THR（发送保持寄存器）→ 写入时 print! 到终端
- [x] LSR（线路状态寄存器）→ 始终返回 0x60（发送空+发送完成）
- [x] RBR（接收缓冲寄存器）→ 读取终端输入
- [x] IER（中断使能）→ 接收中断触发（通过 PLIC IRQ#10）

### 3.5 ELF 加载器
- [x] goblin 0.9 解析 ELF 文件（PT_LOAD 段加载）
- [x] 自动提取 tohost 符号地址（无需手动指定）
- [x] 自动提取 begin_signature/end_signature 符号（arch-test 用）
- [x] BSS 段零填充
- [x] 自动检测 ELF vs raw .bin 格式
- [x] 77/77 ELF 测试通过验证

### 3.6 CLI 功能
- [x] `--tohost <hex>` 手动指定 tohost 地址
- [x] `--signature <file>` 运行后导出签名区域（arch-test 格式，每行 8 位 hex）
- [x] `--cycles <n>` 指定最大执行周期数（默认 10M）
- [x] 向后兼容旧的位置参数格式

---

## 四、启动流程

### 4.1 内嵌最小 SBI
- [x] SBI v0.2+ 基本规范（BASE/TIME/SRST + legacy 兼容）
- [x] sbi_set_timer（设置 S-mode 定时器）
- [x] sbi_console_putchar（串口输出）
- [x] sbi_console_getchar（串口输入，当前无输入返回 -1）
- [x] sbi_shutdown（关机）
- [x] Timer Extension (EID 0x54494D45)
- [ ] IPI Extension (EID 0x735049)（预留）

### 4.2 OpenSBI 加载（可选路径）
- [ ] 加载 OpenSBI fw_jump 固件到 M-mode 起始地址
- [ ] 正确实现 M-mode 使其能运行 OpenSBI

### 4.3 Linux 启动
- [x] 加载 Linux Image/ELF 到 RAM 指定偏移
- [x] 加载/生成 DTB（FDT）
- [x] 加载 initramfs 到 RAM
- [x] 设置 a0 = hartid, a1 = DTB 物理地址
- [x] 切到 S-mode 并跳转到内核入口

---

## 五、调试与测试

### 5.1 测试
- [x] riscv-tests RV32I 全部通过（37/37）
- [x] riscv-tests RV32M 全部通过（8/8）
- [x] riscv-tests RV32A 全部通过（10/10）
- [x] riscv-tests rv32mi 全部通过（16/16）
- [x] riscv-tests rv32si 全部通过（6/6）
- [x] 裸机外设集成测试（7/7）— UART/CLINT/PLIC 手写机器码验证
- [ ] riscv-arch-test 官方合规测试（脚本就绪，待运行）
- [ ] 自定义裸机测试程序

### 5.2 测试基础设施
- [x] `scripts/compile_tests.sh` — 交叉编译 riscv-tests
- [x] `scripts/pre-push` — git pre-push hook，push 前自动 cargo test
- [x] `scripts/run-arch-test.sh` — WSL/Linux 下一键运行 riscv-arch-test
- [x] `config/remur/remur-rv32ima/` — arch-test 框架配置（YAML + 宏 + 链接脚本）

### 5.2 调试工具
- [ ] itrace：指令反汇编追踪（可开关）
- [ ] mtrace：关键地址内存访问记录
- [ ] regtrace：寄存器变化追踪
- [ ] difftest：与 Spike 参考模型逐指令对比（可选）
- [ ] GDB remote stub（可选）

---

## 六、性能基准与优化（M4）

### 6.1 Benchmark 基础设施
- [x] criterion 0.7 集成（`benches/isa_benchmark.rs`）
- [x] Release profile 调优（LTO + codegen-units=1 + opt-level=3）
- [x] 单项测试 benchmark（add/jal/lw/sw/beq/mul/div/amoadd_w/csr/illegal/dirty）
- [x] 全量 77 测试聚合 benchmark

### 6.2 使用方法

```bash
# 运行全部 benchmark（release 模式，含 LTO）
cargo bench

# 只运行特定 benchmark
cargo bench -- "all-77-tests"
cargo bench -- "riscv-test/add"

# 与上次结果对比（criterion 自动保存 baseline）
cargo bench           # 第一次运行 → 保存 baseline
# ... 修改代码 ...
cargo bench           # 自动与上次对比，显示 ±% 变化

# 手动保存/对比 baseline
cargo bench -- --save-baseline before-opt
# ... 做优化 ...
cargo bench -- --baseline before-opt
```

### 6.3 基线数据（2024 基准）
| 测试 | 耗时 | 说明 |
|------|------|------|
| 单项 (add/jal/lw/sw) | ~65-73 µs | 每个测试几百条指令 |
| dirty (Sv32 页表) | ~69 µs | 最复杂的特权测试 |
| **全量 77 测试** | **~5.6 ms** | 77 个测试跑一遍 |

### 6.4 已完成的优化
- [x] Feature flag 框架（`cached-decode`, `software-tlb`）
- [x] 译码缓存（512 条目直接映射，`cached-decode` feature）
- [x] Instruction 结构体 u8 寄存器索引（内存减小 62%）

### 6.5 计划中的优化
- [ ] 二级查表译码（opcode → funct3 → funct7 函数指针数组）
- [ ] 软件 TLB（缓存 VA→PA 映射，减少页表遍历）
- [ ] 基本块缓存（已译码指令序列复用）
- [ ] Host 内存直接映射（减少地址翻译）
- [ ] JIT 编译（长期目标）
