#!/bin/bash
# test-all.sh — 一键全量测试（riscv-tests + riscv-arch-test）
#
# 自动处理：
#   1. clone riscv-tests（如果不存在）
#   2. 编译测试二进制
#   3. 运行 cargo test（84 个用户态/特权态/外设/SBI/Linux 测试）
#   4. clone riscv-arch-test（如果不存在）
#   5. 运行 arch-test 官方合规框架
#
# 依赖：
#   - Rust 工具链（cargo）
#   - riscv64-unknown-elf-gcc
#   - Python 3（arch-test 框架用）
#
# 用法：
#   ./scripts/test-all.sh              # 全量测试
#   ./scripts/test-all.sh --quick      # 仅 cargo test，跳过 arch-test
#   ./scripts/test-all.sh --arch-only  # 仅 arch-test
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

QUICK=false
ARCH_ONLY=false

for arg in "$@"; do
    case $arg in
        --quick) QUICK=true ;;
        --arch-only) ARCH_ONLY=true ;;
        -h|--help)
            echo "Usage: $0 [--quick] [--arch-only]"
            echo "  --quick      仅运行 cargo test，跳过 arch-test"
            echo "  --arch-only  仅运行 riscv-arch-test"
            exit 0
            ;;
    esac
done

echo "╔═══════════════════════════════════════════╗"
echo "║       REMUR 全量测试                      ║"
echo "╚═══════════════════════════════════════════╝"
echo ""

# ── 检查工具链 ─────────────────────────────────────
echo "▶ 检查工具链..."
MISSING=""
command -v cargo >/dev/null 2>&1 || MISSING="$MISSING cargo"
command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 || MISSING="$MISSING riscv64-unknown-elf-gcc"

if [ -n "$MISSING" ]; then
    echo "  ❌ 缺少工具:$MISSING"
    echo "  安装提示:"
    echo "    cargo: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "    riscv64-unknown-elf-gcc: sudo apt install gcc-riscv64-unknown-elf"
    exit 1
fi
echo "  ✅ 工具链就绪"

# ── 获取 riscv-tests 源码 ──────────────────────────
RISCV_TESTS_DIR="$PROJECT_DIR/riscv-tests"

if [ ! -d "$RISCV_TESTS_DIR/isa/rv32ui" ]; then
    echo ""
    echo "▶ 获取 riscv-tests..."
    if [ -d "$RISCV_TESTS_DIR" ]; then
        rm -rf "$RISCV_TESTS_DIR"
    fi
    git clone --depth 1 https://github.com/riscv-software-src/riscv-tests.git "$RISCV_TESTS_DIR"
    echo "  ✅ riscv-tests 已下载"
else
    echo "  ✅ riscv-tests 已存在"
fi

# ── 编译测试二进制 ─────────────────────────────────
BIN_DIR="$PROJECT_DIR/tests/bins"

if [ ! -d "$BIN_DIR" ] || [ -z "$(ls -A "$BIN_DIR" 2>/dev/null)" ]; then
    echo ""
    echo "▶ 编译测试二进制..."
    bash "$SCRIPT_DIR/compile_tests.sh"
    echo "  ✅ 编译完成"
else
    echo "  ✅ 测试二进制已存在（如需重新编译请删除 tests/bins/）"
fi

# ── Part 1: cargo test ─────────────────────────────
if [ "$ARCH_ONLY" = false ]; then
    echo ""
    echo "╭───────────────────────────────────────────╮"
    echo "│  Part 1/2: cargo test（riscv-tests）       │"
    echo "╰───────────────────────────────────────────╯"
    echo ""
    cargo test 2>&1
    echo ""
    echo "  ✅ cargo test 全部通过"
fi

# ── Part 2: arch-test ──────────────────────────────
if [ "$QUICK" = false ]; then
    echo ""
    echo "╭───────────────────────────────────────────╮"
    echo "│  Part 2/2: riscv-arch-test 官方合规测试    │"
    echo "╰───────────────────────────────────────────╯"
    echo ""

    # 检查 Python
    if ! command -v python3 >/dev/null 2>&1; then
        echo "  ⚠️  跳过 arch-test：缺少 python3"
        echo "  安装：sudo apt install python3"
        exit 0
    fi

    bash "$SCRIPT_DIR/run-arch-test.sh" --skip-build
    echo ""
    echo "  ✅ riscv-arch-test 全部通过"
fi

echo ""
echo "╔═══════════════════════════════════════════╗"
echo "║  🎉 全量测试完成                          ║"
echo "╚═══════════════════════════════════════════╝"
