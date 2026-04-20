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
USE_RELEASE=false

# 解析参数
for arg in "$@"; do
    case $arg in
        --skip-build) SKIP_BUILD=true ;;
        --clean) CLEAN=true ;;
        --release) USE_RELEASE=true ;;
        -h|--help)
            echo "Usage: $0 [--skip-build] [--clean] [--release]"
            echo "  --skip-build  Skip building REMUR (use existing binary)"
            echo "  --clean       Clean riscv-arch-test work directory before running"
            echo "  --release     Build and use release binary (default: debug)"
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
    if [ "$USE_RELEASE" = true ]; then
        echo "▶ Step 1: Building REMUR (release)..."
        cd "$PROJECT_DIR"
        cargo build --release --quiet
        echo "  ✅ Build complete (release)"
    else
        echo "▶ Step 1: Building REMUR (debug)..."
        cd "$PROJECT_DIR"
        cargo build --quiet
        echo "  ✅ Build complete (debug)"
    fi
else
    echo ""
    echo "▶ Step 1: Skipped (--skip-build)"
fi

if [ "$USE_RELEASE" = true ]; then
    REMUR_BIN="$PROJECT_DIR/target/release/remur"
else
    REMUR_BIN="$PROJECT_DIR/target/debug/remur"
fi
# Windows 上通过 WSL 运行时，二进制是 .exe
if [ ! -f "$REMUR_BIN" ] && [ -f "${REMUR_BIN}.exe" ]; then
    REMUR_BIN="${REMUR_BIN}.exe"
fi
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

# 如果是 Windows .exe，创建路径转换 wrapper
if [[ "$REMUR_BIN" == *.exe ]]; then
    WRAPPER="$DEST_CONFIG/remur-rv32ima/run_dut.sh"
    cat > "$WRAPPER" << 'WEOF'
#!/bin/bash
# Wrapper: 将 WSL 路径转换为 Windows 路径后调用 remur.exe
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REMUR_EXE="$(cat "$SCRIPT_DIR/run_cmd.txt")"
WIN_PATH="$(wslpath -w "$1")"
"$REMUR_EXE" "$WIN_PATH"
WEOF
    chmod +x "$WRAPPER"
    RUN_CMD="$WRAPPER"
else
    RUN_CMD="$REMUR_BIN"
fi
echo "  ✅ Config installed"

# ── Step 4: Clean (可选) ──────────────────────────
if [ "$CLEAN" = true ]; then
    echo ""
    echo "▶ Step 4: Cleaning work directory..."
    cd "$ARCH_TEST_DIR"
    make clean 2>/dev/null || true
    echo "  ✅ Cleaned"
fi

# ── Step 5: 检查 & 安装依赖 ─────────────────────
echo ""
echo "▶ Step 5: Checking dependencies..."
MISSING=""
command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 || MISSING="$MISSING riscv64-unknown-elf-gcc"
command -v python3 >/dev/null 2>&1 || MISSING="$MISSING python3"

if [ -n "$MISSING" ]; then
    echo "  ❌ Missing:$MISSING"
    echo "  Install: sudo apt install gcc-riscv64-unknown-elf python3"
    exit 1
fi
echo "  ✅ gcc + python3 found"

# uv 或 mise（riscv-arch-test 的 Python 依赖管理）— 可自动装
if ! command -v uv >/dev/null 2>&1 && ! command -v mise >/dev/null 2>&1; then
    echo "  ⚠ Neither uv nor mise found — installing uv..."
    curl -LsSf https://astral.sh/uv/install.sh | sh 2>/dev/null
    export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
    if ! command -v uv >/dev/null 2>&1; then
        echo "  ❌ uv install failed. Install manually: curl -LsSf https://astral.sh/uv/install.sh | sh"
        exit 1
    fi
    echo "  ✅ uv installed"
else
    echo "  ✅ uv/mise found"
fi

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

python3 "$ARCH_TEST_DIR/run_tests.py" "$RUN_CMD" "$ELF_DIR"

echo ""
echo "═══════════════════════════════════════════"
echo "  Done! Check results above."
echo "═══════════════════════════════════════════"
