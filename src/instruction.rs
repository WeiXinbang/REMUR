// ===== 子操作枚举 =====

#[derive(Debug, Clone, Copy)]
pub enum ROp {
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    // RV32M
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
}

#[derive(Debug, Clone, Copy)]
pub enum IOp {
    Addi,
    Slti,
    Sltiu,
    Xori,
    Ori,
    Andi,
}

#[derive(Debug, Clone, Copy)]
pub enum ShiftOp {
    Slli,
    Srli,
    Srai,
}

#[derive(Debug, Clone, Copy)]
pub enum LoadOp {
    Lb,
    Lh,
    Lw,
    Lbu,
    Lhu,
}

#[derive(Debug, Clone, Copy)]
pub enum StoreOp {
    Sb,
    Sh,
    Sw,
}

#[derive(Debug, Clone, Copy)]
pub enum BrOp {
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
}

#[derive(Debug, Clone, Copy)]
pub enum CsrOp {
    Rw,
    Rs,
    Rc,
    Rwi,
    Rsi,
    Rci,
}

#[derive(Debug, Clone, Copy)]
pub enum AmoOp {
    Lr,
    Sc,
    Swap,
    Add,
    Xor,
    And,
    Or,
    Min,
    Max,
    Minu,
    Maxu,
}

// ===== 指令枚举（按格式分组）=====
// 寄存器索引用 u8（0-31），shamt 用 u8（0-31），减小内存占用

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // aq/rl、SfenceVma 的 rs1/rs2 将在后续步骤使用
pub enum Instruction {
    R {
        op: ROp,
        rd: u8,
        rs1: u8,
        rs2: u8,
    },
    I {
        op: IOp,
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Shift {
        op: ShiftOp,
        rd: u8,
        rs1: u8,
        shamt: u8,
    },
    Load {
        op: LoadOp,
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Store {
        op: StoreOp,
        rs1: u8,
        rs2: u8,
        imm: i32,
    },
    Branch {
        op: BrOp,
        rs1: u8,
        rs2: u8,
        imm: i32,
    },
    Lui {
        rd: u8,
        imm: u32,
    },
    Auipc {
        rd: u8,
        imm: u32,
    },
    Jal {
        rd: u8,
        imm: i32,
    },
    Jalr {
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Csr {
        op: CsrOp,
        rd: u8,
        rs1: u8,
        csr: u16,
    },
    // RV32A
    Amo {
        op: AmoOp,
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    Ecall,
    Ebreak,
    Fence,
    Mret,
    Sret,
    Wfi,
    SfenceVma {
        rs1: u8,
        rs2: u8,
    },
    Illegal(u32), // 非法指令，携带原始指令值
}
