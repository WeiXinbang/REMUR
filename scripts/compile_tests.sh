#!/bin/bash
# 编译 riscv-tests (rv32ui/rv32um/rv32ua) 为 flat binary
# 依赖：riscv64-unknown-elf-gcc, 完整的 riscv-tests 源码
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# 如果 riscv-tests 不完整（缺 rv64ui），从上游获取
if [ ! -d "$PROJECT_DIR/riscv-tests/isa/rv64ui" ]; then
    echo "Fetching full riscv-tests from upstream..."
    TMPDIR=$(mktemp -d)
    git clone --depth 1 https://github.com/riscv-software-src/riscv-tests.git "$TMPDIR/riscv-tests"
    cp -rn "$TMPDIR/riscv-tests/isa/"* "$PROJECT_DIR/riscv-tests/isa/" 2>/dev/null || true
    rm -rf "$TMPDIR"
fi

ISA_DIR="$PROJECT_DIR/riscv-tests/isa"
ENV_DIR="$PROJECT_DIR/riscv-tests/env"
OUT_DIR="$PROJECT_DIR/tests/bins"
mkdir -p "$OUT_DIR"

CC=riscv64-unknown-elf-gcc
OBJCOPY=riscv64-unknown-elf-objcopy
INCLUDES="-I$ENV_DIR/p -I$ENV_DIR -I$ISA_DIR/macros/scalar"
BASE_FLAGS="-mabi=ilp32 -static -nostdlib -nostartfiles"

PASS=0
FAIL=0

compile_suite() {
    local suite=$1   # e.g. rv32ui
    local march=$2   # e.g. rv32i_zicsr
    shift 2
    local tests="$@"

    for t in $tests; do
        local SRC="$ISA_DIR/$suite/$t.S"
        local ELF="/tmp/${suite}-p-$t"
        local BIN="$OUT_DIR/${suite}-p-$t.bin"

        if $CC $BASE_FLAGS -march=$march -T "$ENV_DIR/p/link.ld" $INCLUDES "$SRC" -o "$ELF" 2>/dev/null; then
            $OBJCOPY -O binary "$ELF" "$BIN"
            PASS=$((PASS + 1))
        else
            echo "FAIL to compile: $suite/$t"
            FAIL=$((FAIL + 1))
        fi
    done
}

# RV32I
compile_suite rv32ui rv32i_zicsr \
    add addi and andi auipc \
    beq bge bgeu blt bltu bne \
    jal jalr \
    lb lbu lh lhu lw lui \
    or ori sb sh sw \
    sll slli slt slti sltiu sltu \
    sra srai srl srli sub xor xori

# RV32M
compile_suite rv32um rv32im_zicsr \
    mul mulh mulhsu mulhu div divu rem remu

# RV32A
compile_suite rv32ua rv32ima_zicsr \
    amoadd_w amoand_w amomax_w amomaxu_w \
    amomin_w amominu_w amoor_w amoswap_w amoxor_w lrsc

echo "Compiled $PASS tests ($FAIL failures)"
[ $FAIL -eq 0 ] || exit 1
