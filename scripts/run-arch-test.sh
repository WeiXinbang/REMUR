#!/bin/bash
# run-arch-test.sh — 在 WSL/Linux 下运行 riscv-arch-test 合规测试
#
# 前置依赖:
#   - Rust 工具链 (cargo)
#   - riscv64-unknown-elf-gcc (交叉编译器)
#   - Python 3.8+ (pip install uv, 或使用 mise)
#   - sail_riscv_sim (参考模型, 可选)
#
# 用法:
#   ./scripts/run-arch-test.sh              # 运行全部测试
#   ./scripts/run-arch-test.sh --skip-build # 跳过编译 REMUR
#   ./scripts/run-arch-test.sh --clean      # 清理后重新开始
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ARCH_TEST_DIR="$PROJECT_DIR/riscv-arch-test"
REMUR_CONFIG_DIR="$PROJECT_DIR/config/remur"
SKIP_BUILD=false
CLEAN=false

# 解析参数
for arg in "$@"; do
    case $arg in
        --skip-build) SKIP_BUILD=true ;;
        --clean) CLEAN=true ;;
        -h|--help)
            echo "Usage: $0 [--skip-build] [--clean]"
            echo "  --skip-build  Skip building REMUR (use existing binary)"
            echo "  --clean       Clean riscv-arch-test work directory before running"
            exit 0
            ;;
    esac
done

echo "═══════════════════════════════════════════"
echo "  REMUR riscv-arch-test Runner"
echo "═══════════════════════════════════════════"

# ── Step 1: 编译 REMUR ──────────────────────────
if [ "$SKIP_BUILD" = false ]; then
    echo ""
    echo "▶ Step 1: Building REMUR (release)..."
    cd "$PROJECT_DIR"
    cargo build --release --quiet
    echo "  ✅ Build complete"
else
    echo ""
    echo "▶ Step 1: Skipped (--skip-build)"
fi

REMUR_BIN="$PROJECT_DIR/target/release/remur"
if [ ! -f "$REMUR_BIN" ]; then
    echo "  ❌ REMUR binary not found at $REMUR_BIN"
    echo "     Run without --skip-build first."
    exit 1
fi

# ── Step 2: 获取 riscv-arch-test ──────────────────
echo ""
echo "▶ Step 2: Checking riscv-arch-test..."
if [ ! -d "$ARCH_TEST_DIR" ]; then
    echo "  Cloning riscv-arch-test..."
    git clone --depth 1 https://github.com/riscv-non-isa/riscv-arch-test.git "$ARCH_TEST_DIR"
else
    echo "  ✅ Already present"
fi

# ── Step 3: 注入 REMUR 配置 ──────────────────────
echo ""
echo "▶ Step 3: Setting up REMUR config..."
DEST_CONFIG="$ARCH_TEST_DIR/config/remur"
mkdir -p "$DEST_CONFIG"
cp -r "$REMUR_CONFIG_DIR"/* "$DEST_CONFIG/"

# 更新 run_cmd.txt 使用绝对路径
echo "$REMUR_BIN" > "$DEST_CONFIG/remur-rv32ima/run_cmd.txt"
echo "  ✅ Config installed"

# ── Step 4: Clean (可选) ──────────────────────────
if [ "$CLEAN" = true ]; then
    echo ""
    echo "▶ Step 4: Cleaning work directory..."
    cd "$ARCH_TEST_DIR"
    make clean 2>/dev/null || true
    echo "  ✅ Cleaned"
fi

# ── Step 5: 检查依赖 ──────────────────────────────
echo ""
echo "▶ Step 5: Checking dependencies..."
MISSING=""
command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 || MISSING="$MISSING riscv64-unknown-elf-gcc"
command -v python3 >/dev/null 2>&1 || MISSING="$MISSING python3"

if [ -n "$MISSING" ]; then
    echo "  ❌ Missing tools:$MISSING"
    echo ""
    echo "  Install guide:"
    echo "    riscv64-unknown-elf-gcc: sudo apt install gcc-riscv64-unknown-elf"
    echo "    python3: sudo apt install python3 python3-pip"
    exit 1
fi
echo "  ✅ All dependencies found"

# ── Step 6: 生成测试 + 编译 ELF ──────────────────
echo ""
echo "▶ Step 6: Generating and compiling tests..."
cd "$ARCH_TEST_DIR"

# 只生成 I/M/A 扩展的测试
make elfs CONFIG_FILES="config/remur/remur-rv32ima/test_config.yaml" \
    EXTENSIONS="I,M,Zaamo,Zalrsc" \
    FAST=1

echo "  ✅ ELFs built"

# ── Step 7: 运行测试 ──────────────────────────────
echo ""
echo "▶ Step 7: Running tests with REMUR..."
ELF_DIR="$ARCH_TEST_DIR/work/remur-rv32ima/elfs"

if [ ! -d "$ELF_DIR" ] || [ -z "$(ls -A "$ELF_DIR" 2>/dev/null)" ]; then
    echo "  ❌ No ELF files found in $ELF_DIR"
    exit 1
fi

python3 "$ARCH_TEST_DIR/run_tests.py" "$REMUR_BIN" "$ELF_DIR"

echo ""
echo "═══════════════════════════════════════════"
echo "  Done! Check results above."
echo "═══════════════════════════════════════════"
