// ===== 子操作枚举 =====

#[derive(Debug, Clone, Copy)]
pub enum ROp {
    Add, Sub, Sll, Slt, Sltu, Xor, Srl, Sra, Or, And,
    // RV32M
    Mul, Mulh, Mulhsu, Mulhu, Div, Divu, Rem, Remu,
}

#[derive(Debug, Clone, Copy)]
pub enum IOp { Addi, Slti, Sltiu, Xori, Ori, Andi }

#[derive(Debug, Clone, Copy)]
pub enum ShiftOp { Slli, Srli, Srai }

#[derive(Debug, Clone, Copy)]
pub enum LoadOp { Lb, Lh, Lw, Lbu, Lhu }

#[derive(Debug, Clone, Copy)]
pub enum StoreOp { Sb, Sh, Sw }

#[derive(Debug, Clone, Copy)]
pub enum BrOp { Beq, Bne, Blt, Bge, Bltu, Bgeu }

#[derive(Debug, Clone, Copy)]
pub enum CsrOp { Rw, Rs, Rc, Rwi, Rsi, Rci }

#[derive(Debug, Clone, Copy)]
pub enum AmoOp { Lr, Sc, Swap, Add, Xor, And, Or, Min, Max, Minu, Maxu }

// ===== 指令枚举（按格式分组）=====

#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    R     { op: ROp,     rd: usize, rs1: usize, rs2: usize },
    I     { op: IOp,     rd: usize, rs1: usize, imm: i32 },
    Shift { op: ShiftOp, rd: usize, rs1: usize, shamt: u32 },
    Load  { op: LoadOp,  rd: usize, rs1: usize, imm: i32 },
    Store { op: StoreOp, rs1: usize, rs2: usize, imm: i32 },
    Branch{ op: BrOp,    rs1: usize, rs2: usize, imm: i32 },
    Lui   { rd: usize, imm: u32 },
    Auipc { rd: usize, imm: u32 },
    Jal   { rd: usize, imm: i32 },
    Jalr  { rd: usize, rs1: usize, imm: i32 },
    Csr   { op: CsrOp,  rd: usize, rs1: usize, csr: u16 },
    // RV32A
    Amo   { op: AmoOp,  rd: usize, rs1: usize, rs2: usize, aq: bool, rl: bool },
    Ecall,
    Ebreak,
    Fence,
    Mret,
    Sret,
    Wfi,
    SfenceVma { rs1: usize, rs2: usize },
}
