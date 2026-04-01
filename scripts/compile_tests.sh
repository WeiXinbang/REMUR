#!/bin/bash
# 编译 rv32ui riscv-tests 为 flat binary
# 依赖：riscv64-unknown-elf-gcc, 完整的 riscv-tests 源码
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# 如果 riscv-tests 不完整（缺 rv64ui），从上游获取
if [ ! -d "$PROJECT_DIR/riscv-tests/isa/rv64ui" ]; then
    echo "Fetching full riscv-tests from upstream..."
    TMPDIR=$(mktemp -d)
    git clone --depth 1 https://github.com/riscv-software-src/riscv-tests.git "$TMPDIR/riscv-tests"
    # 只复制 isa 目录中缺失的部分
    cp -rn "$TMPDIR/riscv-tests/isa/"* "$PROJECT_DIR/riscv-tests/isa/" 2>/dev/null || true
    rm -rf "$TMPDIR"
fi

ISA_DIR="$PROJECT_DIR/riscv-tests/isa"
ENV_DIR="$PROJECT_DIR/riscv-tests/env"
OUT_DIR="$PROJECT_DIR/tests/bins"
mkdir -p "$OUT_DIR"

TESTS="add addi and andi auipc beq bge bgeu blt bltu bne jal jalr lb lbu lh lhu lw lui or ori sb sh sw sll slli slt slti sltiu sltu sra srai srl srli sub xor xori"

CC=riscv64-unknown-elf-gcc
OBJCOPY=riscv64-unknown-elf-objcopy
FLAGS="-march=rv32i_zicsr -mabi=ilp32 -static -nostdlib -nostartfiles"
INCLUDES="-I$ENV_DIR/p -I$ENV_DIR -I$ISA_DIR/macros/scalar"

PASS=0
FAIL=0

for t in $TESTS; do
    SRC="$ISA_DIR/rv32ui/$t.S"
    ELF="/tmp/rv32ui-p-$t"
    BIN="$OUT_DIR/rv32ui-p-$t.bin"

    if $CC $FLAGS -T "$ENV_DIR/p/link.ld" $INCLUDES "$SRC" -o "$ELF" 2>/dev/null; then
        $OBJCOPY -O binary "$ELF" "$BIN"
        PASS=$((PASS + 1))
    else
        echo "FAIL to compile: $t"
        FAIL=$((FAIL + 1))
    fi
done

echo "Compiled $PASS tests ($FAIL failures)"
[ $FAIL -eq 0 ] || exit 1
