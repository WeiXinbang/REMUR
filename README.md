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
| M7: 调试/测试    | ✅ M7.2 已完成 | itrace + difftest + 外部参考 trace 生成命令链路 |
| M8: 高级优化(可选) | ⏸ 暂缓 | 基本块缓存/JIT，等 M7 稳定后再做 |

## 测试结果

`cargo test` 现在覆盖：

- riscv-tests 用户态/特权测试：`rv32ui` / `rv32um` / `rv32ua` / `rv32mi` / `rv32si`
- 裸机外设集成测试：`peripherals`
- SBI / Linux 启动最小闭环测试
- riscv-arch-test 合规测试（自动 clone + 编译 + 签名对比）

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
  remur <binary_or_elf> [--tohost <hex>] [--signature <file>] [--cycles <n>] [--no-limit]
        [--tui] [--itrace] [--itrace-file <file>] [--itrace-limit <n>]
        [--difftest-ref <file>] [--difftest-ref-cmd <cmd>] [--difftest-ref-out <file>]

Linux 模式:
  remur linux [--kernel <image_or_elf>] [--dtb <file>] [--initramfs <file>]
        [--kernel-addr <hex>] [--dtb-addr <hex>] [--initramfs-addr <hex>]
        [--bootargs <string>] [--cycles <n>] [--no-limit] [--tui]
        [--itrace] [--itrace-file <file>] [--itrace-limit <n>]
        [--difftest-ref <file>] [--difftest-ref-cmd <cmd>] [--difftest-ref-out <file>]

Linux 模式（兼容）:
  remur --linux --kernel <image_or_elf> [--dtb <file>] [--initramfs <file>]
        [--kernel-addr <hex>] [--dtb-addr <hex>] [--initramfs-addr <hex>]
        [--bootargs <string>] [--cycles <n>] [--no-limit]

TUI 仪表盘（需要 --features tui 编译）:
  remur linux --tui              # Linux 模式 TUI
  remur test.elf --tui           # 普通模式 TUI
```

也可以用脚本：

```powershell
.\scripts\run_linux.ps1
# 或指定自备内核
.\scripts\run_linux.ps1 -Kernel path\to\Image
```

### 测试命令说明

```bash
# 全量测试（riscv-tests + arch-test 合规 + 外设 + SBI + Linux 烟雾）
cargo test

# 环境要求：WSL + riscv64-unknown-elf-gcc + python3
# 首次运行会自动 clone 并编译所有测试二进制

# 单独跑 arch-test（等价于 cargo test --test arch_framework）
cargo arch-test
```

### TUI 仪表盘

展示 CPU 寄存器、CSR、UART 输出的终端仪表盘，适合视频演示：

```bash
# 构建时启用 TUI feature
cargo build --release --features tui

# Linux 模式 TUI
cargo run --features tui -- linux --tui

# 普通模式 TUI
cargo run --features tui -- tests/bins/rv32ui-p-add.elf --tui
```

常用操作：

- `Space`：暂停 / 继续
- `F1`：切换控制模式 / 输入模式
- `n` / `N` / `m` / `M`：暂停时单步执行 1 / 10 / 100 / 1000 条指令
- 输入模式下键盘直接发到 UART，`Ctrl+C` 会转发给 Linux；要退出 TUI 先按 `F1` 或 `Esc` 回控制模式

### M5 架构测试（riscv-arch-test）

- `cargo test` 会自动运行 arch-test 合规测试（含 clone 仓库 + 编译 ELF + 签名对比）。
- 环境要求：WSL + `riscv64-unknown-elf-*` 交叉工具链 + Python 3。
- 也可单独运行：`cargo arch-test`。

### M6 Linux 启动现状（含 ls 说明）

- `cargo run linux`（零配置）会自动下载并缓存预构建 `Image/rootfs`，默认用 4MiB 对齐地址启动（`0x80400000`）。
- 默认 bootargs 会进入 `rdinit=/bin/sh`，可直接在串口里输入 `ls`。
- 当前可看到 BusyBox shell 提示符并执行命令（会提示 `can't access tty; job control turned off`，但不影响 `ls`/`echo` 等基本交互）。
- 直接进 shell 时通常还没挂载 procfs；需要手动执行 `mount -t proc proc /proc`。
- 若在 shell 里执行关机，推荐用 `poweroff -f`，这样会直接走 SBI shutdown 路径并让 TUI 停在 `SBI shutdown` 状态。
- 若命令行里退格只移动光标、不擦除字符，可先执行 `export TERM=vt100`。

### M7 / M8 是否现在开做

- **M7.2 已落地**：`--itrace`/`--itrace-file`/`--itrace-limit` 可输出指令级 trace；`--difftest-ref` / `--difftest-ref-cmd` / `--difftest-ref-out` 已形成最小对拍闭环。
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

# 一步完成：先调用外部命令生成参考 trace，再自动对拍（Linux/WSL）
cargo run -- tests/bins/rv32ui-p-add.elf --difftest-ref-cmd "bash scripts/gen_spike_ref.sh tests/bins/rv32ui-p-add.elf" --difftest-ref-out target/trace/add.spike.ref
```

## 项目结构

```
src/
├── main.rs          # CLI 参数解析、模式分发、签名导出
├── linux_boot.rs    # Linux 下载/布局/装载/headless/TUI 路由
├── debug_trace.rs   # itrace / difftest / 参考 trace 生成
├── tui.rs           # 终端仪表盘（feature = "tui"）
├── cpu.rs           # Hart：CSR、trap、Sv32、SBI、step 主循环
├── bus.rs           # 设备路由
├── clint.rs         # 定时器 / 软件中断
├── plic.rs          # 外部中断控制器
├── uart.rs          # UART 16550 子集
├── dtb.rs           # 默认 / rv32emu 兼容 DTB 生成
├── loader.rs        # ELF/bin 加载器
├── decode.rs        # 指令译码
├── instruction.rs   # 指令枚举 + 子操作枚举
├── memory.rs        # 物理内存（可配置基址）
├── lib.rs           # bench / 测试复用导出
└── execute/
    └── mod.rs       # 指令执行（RV32I/M/A/Zicsr）
tests/
├── common/          # 共享测试辅助函数
├── bins/            # 预编译的 riscv-tests 二进制
├── rv32ui.rs        # RV32I 集成测试
├── rv32um.rs        # RV32M 集成测试
├── rv32ua.rs        # RV32A 集成测试
├── rv32mi.rs        # M-mode 特权测试
├── rv32si.rs        # S-mode 特权测试
├── peripherals.rs   # UART / CLINT / PLIC 集成测试
├── sbi.rs           # SBI 行为测试
├── linux_boot.rs    # Linux 启动最小闭环测试
└── arch_framework.rs# riscv-arch-test 合规框架
scripts/
├── compile_tests.sh # 编译 riscv-tests 的脚本
├── setup_tests.ps1  # Windows 下准备测试二进制
├── run_linux.ps1    # Linux 启动包装脚本
├── run-arch-test.sh # arch-test 执行入口
├── gen_spike_ref.sh # 生成 Spike 参考 trace
└── spike_to_remur_trace.py # Spike -> REMUR trace 格式转换
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
