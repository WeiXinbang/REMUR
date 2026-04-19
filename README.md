# REMUR — RISC-V Emulator in Rust

用 Rust 从零实现的 RV32IMA 指令集模拟器，目标是启动 Linux。

## 当前进度

| 里程碑 | 状态 | 说明 |
|--------|------|------|
| M1: RV32I 解释器 | ✅ 完成 | 47 条基础指令 + Zicsr + trap/mret |
| M2: 扩展指令集   | ✅ 完成 | RV32M 乘除法 + RV32A 原子操作 |
| M3: 特权架构     | ✅ 完成 | M/S/U 模式 + 异常委托 + Sv32 页表 |
| M4: 性能优化     | ✅ 完成 | criterion + 译码缓存 + u8 寄存器索引 |
| M5: SoC 外设     | ✅ 完成 | CLINT + PLIC + UART + ELF 加载器 + arch-test 适配 |
| M6: 启动 Linux   | ✅ 完成 | 内嵌 SBI + DTB(可生成) + 内核/initramfs 加载 |
| M7: 调试/测试    | ✅ M7.1 已完成 | itrace + 最小 difftest（参考 trace 对拍） |
| M8: 高级优化(可选) | ⏸ 暂缓 | 基本块缓存/JIT，等 M7 稳定后再做 |

## 测试结果

```
rv32ui-p (RV32I):  37/37 ✅
rv32um-p (RV32M):   8/8  ✅
rv32ua-p (RV32A):  10/10 ✅
rv32mi-p (M-mode): 16/16 ✅
rv32si-p (S-mode):  6/6  ✅
peripherals:        7/7  ✅
─────────────────────────
总计:              84/84 ✅
```

## 快速开始

```bash
# 构建
cargo build --release

# 运行裸机二进制（如求和程序）
cargo run -- sum.bin

# 运行 riscv-tests（需先编译测试二进制）
cargo run -- tests/bins/rv32ui-p-add.bin --tohost 80001000

# 运行全部集成测试
cargo test

# M6: Linux 启动（内嵌 SBI）
cargo run -- --linux --kernel path/to/Image --cycles 50000000

# 等价简写（新增）
cargo run linux --kernel path/to/Image --cycles 50000000

# Cargo alias（新增）
cargo linux --kernel path/to/Image --cycles 50000000

# 零配置模式（自动下载预构建 Linux Image + rootfs 后启动）
cargo run linux

# 推荐：显示早期串口日志（并使用 4MiB 对齐内核地址）
cargo run linux --kernel-addr 0x80400000 --bootargs "earlycon=uart8250,mmio,0x10000000 console=ttyS0"

# 验证 userspace ls 已可执行（ls 作为 init 运行后会退出并触发 panic，属于预期）
cargo run linux --kernel-addr 0x80400000 --bootargs "earlycon=uart8250,mmio,0x10000000 console=ttyS0 rdinit=/bin/ls"
```

### CLI 参数

```text
普通模式:
  remur <binary_or_elf> [--tohost <hex>] [--signature <file>] [--cycles <n>]
        [--itrace] [--itrace-file <file>] [--itrace-limit <n>] [--difftest-ref <file>]

Linux 模式:
  remur linux [--kernel <image_or_elf>] [--dtb <file>] [--initramfs <file>]
        [--kernel-addr <hex>] [--dtb-addr <hex>] [--initramfs-addr <hex>]
        [--bootargs <string>] [--cycles <n>]
        [--itrace] [--itrace-file <file>] [--itrace-limit <n>] [--difftest-ref <file>]

Linux 模式（兼容）:
  remur --linux --kernel <image_or_elf> [--dtb <file>] [--initramfs <file>]
        [--kernel-addr <hex>] [--dtb-addr <hex>] [--initramfs-addr <hex>]
        [--bootargs <string>] [--cycles <n>]
```

也可以用脚本：

```powershell
.\scripts\run_linux.ps1
# 或指定自备内核
.\scripts\run_linux.ps1 -Kernel path\to\Image
```

### 测试命令说明

```bash
# 默认：集成测试（riscv-tests + 外设 + Linux 启动烟雾）
cargo test

# 若 tests/bins 缺失，会自动拉源码并编译 riscv-tests

# 运行 riscv-arch-test 官方框架（耗时长，默认不随 cargo test 执行）
cargo arch-test
```

### M5 架构测试怎么用（riscv-arch-test）

- `cargo test`：默认只跑仓库内快速/常规测试，不包含 arch framework。
- `cargo arch-test`：运行 `tests/arch_framework.rs`（`#[ignore]`）触发官方框架流程。
- 环境要求：Linux/WSL + `riscv64-unknown-elf-*` 交叉工具链 + Python 依赖（由 `scripts/run-arch-test.sh` 使用）。
- 结论：配置已接好，命令入口也已接好；是否能完整跑完取决于本机 Linux/WSL 工具链环境。

### M6 Linux 启动现状（含 ls 说明）

- `cargo run linux`（零配置）会自动下载并缓存预构建 `Image/rootfs`，默认用 4MiB 对齐地址启动（`0x80400000`）。
- 默认 bootargs 会进入 `rdinit=/bin/sh`，可直接在串口里输入 `ls`。
- 当前可看到 BusyBox shell 提示符并执行命令（会提示 `can't access tty; job control turned off`，但不影响 `ls`/`echo` 等基本交互）。

### M7 / M8 是否现在开做

- **M7.1 已落地**：`--itrace`/`--itrace-file`/`--itrace-limit` 可输出指令级 trace；`--difftest-ref` 可按事件逐步对拍参考 trace。
- **M7.2 下一步**：把参考 trace 生成流程接到外部参考模型（如 Spike）启动片段。
- **建议暂缓 M8**：当前瓶颈还不是解释器吞吐，过早上基本块缓存/JIT 会放大调试成本；等 M7 稳定后再做优化更稳妥。
- **进入 M8 的门槛**：M7 工具可稳定复现问题、Linux 启动回归可自动化、再开始做性能 A/B（如 `cargo bench` 基线对比）。

### M7 调试命令示例

```bash
# 输出到 stderr（最多 200 条）
cargo run -- linux --itrace --itrace-limit 200

# 先生成参考 trace
cargo run -- tests/bins/rv32ui-p-add.elf --itrace --itrace-file target\trace\add.ref

# 再用同一 workload 做 difftest 对拍
cargo run -- tests/bins/rv32ui-p-add.elf --difftest-ref target\trace\add.ref
```

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
