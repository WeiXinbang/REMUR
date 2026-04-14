# REMUR — RISC-V Emulator in Rust

用 Rust 从零实现的 RV32IMA 指令集模拟器，目标是启动 Linux。

## 当前进度

| 里程碑 | 状态 | 说明 |
|--------|------|------|
| M1: RV32I 解释器 | ✅ 完成 | 47 条基础指令 + Zicsr + trap/mret |
| M2: 扩展指令集   | ✅ 完成 | RV32M 乘除法 + RV32A 原子操作 |
| M3: 特权架构     | 🔲 待开始 | M/S/U 模式 + Sv32 页表 |
| M4: SoC 外设     | 🔲 待开始 | CLINT + PLIC + UART |
| M5: 启动 Linux   | 🔲 待开始 | SBI + DTB + 内核加载 |

## 测试结果

```
rv32ui-p (RV32I):  37/37 ✅
rv32um-p (RV32M):   8/8  ✅
rv32ua-p (RV32A):  10/10 ✅
─────────────────────────
总计:              55/55 ✅
```

## 快速开始

```bash
# 构建
cargo build --release

# 运行裸机二进制（如求和程序）
cargo run -- sum.bin

# 运行 riscv-tests（需先编译测试二进制）
cargo run -- tests/bins/rv32ui-p-add.bin 80000000 80001000

# 运行全部集成测试
cargo test
```

### CLI 参数

```
remur <binary_file> [base_addr_hex] [tohost_addr_hex]
```

- `binary_file`: flat binary 文件路径
- `base_addr_hex`: 加载基址（默认 0x80000000）
- `tohost_addr_hex`: riscv-tests 的 tohost 地址（用于 PASS/FAIL 检测）

## 项目结构

```
src/
├── main.rs          # CLI 入口
├── lib.rs           # 库导出（供集成测试用）
├── cpu.rs           # Hart：寄存器、CSR、trap/mret
├── decode.rs        # 指令译码
├── instruction.rs   # 指令枚举 + 子操作枚举
├── memory.rs        # 物理内存（可配置基址）
└── execute/
    └── mod.rs       # 指令执行（RV32I/M/A/Zicsr）
tests/
├── bins/            # 预编译的 riscv-tests 二进制
├── rv32ui.rs        # RV32I 集成测试 (37 cases)
├── rv32um.rs        # RV32M 集成测试 (8 cases)
└── rv32ua.rs        # RV32A 集成测试 (10 cases)
scripts/
└── compile_tests.sh # 编译 riscv-tests 的脚本
```

## 架构设计

采用紧凑的枚举架构，指令用 `Instruction` 枚举 + 子操作枚举（`ROp`、`IOp` 等）表示，
18 个枚举变体覆盖全部 RV32IMA 指令，代码量紧凑。

## 编译 riscv-tests

需要在 Linux/WSL 环境下安装交叉工具链：

```bash
# Ubuntu/Debian
sudo apt install gcc-riscv64-unknown-elf

# 编译全部测试
bash scripts/compile_tests.sh
```

## License

[AGPL-3.0](LICENSE)
