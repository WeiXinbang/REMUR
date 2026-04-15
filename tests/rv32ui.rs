mod common;

macro_rules! riscv_tests {
    ($prefix:expr, $($name:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                common::run_test(&format!("{}-{}", $prefix, stringify!($name)));
            }
        )+
    };
}

riscv_tests! { "rv32ui-p",
    add, addi, and, andi, auipc,
    beq, bge, bgeu, blt, bltu, bne,
    jal, jalr,
    lb, lbu, lh, lhu, lw, lui,
    or, ori,
    sb, sh, sw,
    sll, slli, slt, slti, sltiu, sltu,
    sra, srai, srl, srli,
    sub, xor, xori,
}
