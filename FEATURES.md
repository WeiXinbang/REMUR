# REMUR — RV32IMA 指令集模拟器 功能清单

> **目标**：用 Rust 实现一个 RV32IMAZicsr_Zifencei 指令集模拟器，最终能启动 Linux。
> **架构预留**：多核（Hart）扩展、外设 trait 化。

---

## 一、CPU 核心模块

### 1.1 寄存器组
- [ ] 32 个 32 位通用寄存器 `x0`–`x31`（`x0` 硬连线为 0）
- [ ] 32 位程序计数器 `PC`
- [ ] CSR 寄存器存储（哈希表或数组，按地址索引）

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
- [ ] M-mode（机器态，最高特权，复位入口）
- [ ] S-mode（监管态，Linux 内核运行于此）
- [ ] U-mode（用户态，应用程序运行于此）
- [ ] MRET/SRET 指令（从 trap 返回，切换特权级）
- [ ] WFI 指令（等待中断）

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
- [ ] 指令地址未对齐（Instruction address misaligned）
- [ ] 指令访问错误（Instruction access fault）
- [ ] 非法指令（Illegal instruction）
- [ ] 断点（Breakpoint）
- [ ] 加载地址未对齐（Load address misaligned）
- [ ] 加载访问错误（Load access fault）
- [ ] 存储地址未对齐（Store address misaligned）
- [ ] 存储访问错误（Store access fault）
- [ ] 环境调用 U-mode（Environment call from U-mode）
- [ ] 环境调用 S-mode（Environment call from S-mode）
- [ ] 环境调用 M-mode（Environment call from M-mode）
- [ ] 取指页面错误（Instruction page fault）
- [ ] 加载页面错误（Load page fault）
- [ ] 存储页面错误（Store page fault）

### 2.5 中断处理
- [ ] M-mode 软件中断（MSI）
- [ ] M-mode 定时器中断（MTI）
- [ ] M-mode 外部中断（MEI）
- [ ] S-mode 软件中断（SSI）
- [ ] S-mode 定时器中断（STI）
- [ ] S-mode 外部中断（SEI）
- [ ] 中断委托机制（medeleg/mideleg）
- [ ] 中断优先级判断

### 2.6 Sv32 虚拟内存
- [ ] 两级页表遍历（VPN[1] → VPN[0] → PPN）
- [ ] 4KB 页面大小
- [ ] 4MB 超级页（Megapage）支持
- [ ] PTE 权限位检查（V/R/W/X/U/A/D）
- [ ] 自动设置 A/D 位
- [ ] 页面错误生成
- [ ] TLB 缓存（直接映射，加速翻译）
- [ ] SFENCE.VMA 指令（刷新 TLB）

---

## 三、SoC 外设

### 3.1 总线与地址空间
- [ ] 统一 MMIO 分发框架
- [ ] `Device` trait（统一 read/write 接口）
- [ ] 地址区间注册与查找

#### 默认地址映射（可配置）
| 设备 | 起始地址 | 大小 |
|------|----------|------|
| CLINT   | 0x0200_0000 | 64KB |
| PLIC    | 0x0C00_0000 | 64MB |
| UART0   | 0x1000_0000 | 4KB |
| RAM     | 0x8000_0000 | 128MB+ |

### 3.2 CLINT（Core Local Interruptor）
- [ ] mtime 寄存器（64 位实时计数器）
- [ ] mtimecmp 寄存器（定时器比较值）
- [ ] 定时器中断产生（mtime >= mtimecmp 时触发 MTI）
- [ ] MSIP 寄存器（软件中断触发）

### 3.3 PLIC（极简版，仅支持 UART 中断）
- [ ] source 10 (UART) 优先级寄存器
- [ ] context 0 中断使能寄存器（bit10 控制 UART）
- [ ] context 0 优先级阈值寄存器
- [ ] context 0 claim/complete 寄存器
- [ ] 其余地址读返回 0、写忽略

### 3.4 UART（极简版）
- [ ] THR（发送保持寄存器）→ 写入时输出到终端
- [ ] LSR（线路状态寄存器）→ 始终返回 0x60（发送空+发送完成）
- [ ] RBR（接收缓冲寄存器）→ 读取终端输入（可选，后期加）
- [ ] IER（中断使能）→ 接收中断触发（可选，后期加）
- [ ] 其余寄存器（LCR/MCR/DLL/DLM）读返回 0、写忽略

---

## 四、启动流程

### 4.1 内嵌最小 SBI
- [ ] SBI v0.2+ 基本规范
- [ ] sbi_set_timer（设置 S-mode 定时器）
- [ ] sbi_console_putchar（串口输出）
- [ ] sbi_console_getchar（串口输入）
- [ ] sbi_shutdown（关机）
- [ ] Timer Extension (EID 0x54494D45)
- [ ] IPI Extension (EID 0x735049)（预留）

### 4.2 OpenSBI 加载（可选路径）
- [ ] 加载 OpenSBI fw_jump 固件到 M-mode 起始地址
- [ ] 正确实现 M-mode 使其能运行 OpenSBI

### 4.3 Linux 启动
- [ ] 加载 Linux Image 到 RAM 指定偏移
- [ ] 加载/生成 DTB（FDT）
- [ ] 加载 initramfs 到 RAM
- [ ] 设置 a0 = hartid, a1 = DTB 物理地址
- [ ] 跳转到内核入口

---

## 五、调试与测试

### 5.1 测试
- [ ] riscv-tests RV32I 全部通过
- [ ] riscv-tests RV32M 全部通过
- [ ] riscv-tests RV32A 全部通过
- [ ] riscv-tests privilege 测试通过
- [ ] 自定义裸机测试程序

### 5.2 调试工具
- [ ] itrace：指令反汇编追踪（可开关）
- [ ] mtrace：关键地址内存访问记录
- [ ] regtrace：寄存器变化追踪
- [ ] difftest：与 Spike 参考模型逐指令对比（可选）
- [ ] GDB remote stub（可选）

---

## 六、性能优化（Linux 跑通后）

- [ ] 二级查表译码（opcode → funct3 → funct7 函数指针数组）
- [ ] TLB 命中路径优化
- [ ] 基本块缓存（已译码指令序列复用）
- [ ] Host 内存直接映射（减少地址翻译）
- [ ] JIT 编译（长期目标）
