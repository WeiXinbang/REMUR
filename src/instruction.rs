/// R-type 格式：rd = f(rs1, rs2)
#[derive(Debug, Clone, Copy)]
pub struct RType {
    pub rd: usize,
    pub rs1: usize,
    pub rs2: usize,
}

/// I-type 格式：rd = f(rs1, imm)
#[derive(Debug, Clone, Copy)]
pub struct IType {
    pub rd: usize,
    pub rs1: usize,
    pub imm: i32,
}

/// S-type 格式：mem[rs1 + imm] = rs2
#[derive(Debug, Clone, Copy)]
pub struct SType {
    pub rs1: usize,
    pub rs2: usize,
    pub imm: i32,
}

/// B-type 格式：if f(rs1, rs2) then PC += imm
#[derive(Debug, Clone, Copy)]
pub struct BType {
    pub rs1: usize,
    pub rs2: usize,
    pub imm: i32,
}

/// U-type 格式：rd = imm（高 20 位）
#[derive(Debug, Clone, Copy)]
pub struct UType {
    pub rd: usize,
    pub imm: u32,
}

/// J-type 格式：rd = PC+4, PC += imm
#[derive(Debug, Clone, Copy)]
pub struct JType {
    pub rd: usize,
    pub imm: i32,
}

/// 移位指令：rd = rs1 shift shamt
#[derive(Debug, Clone, Copy)]
pub struct ShiftType {
    pub rd: usize,
    pub rs1: usize,
    pub shamt: u32,
}

/// CSR 指令格式
#[derive(Debug, Clone, Copy)]
pub struct CsrType {
    pub rd: usize,
    pub rs1: usize, // 寄存器源 或 5 位立即数（uimm）
    pub csr: u16,    // 12 位 CSR 地址
}

/// 所有已解码的指令
#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    // ===== RV32I: R-type 算术 =====
    Add(RType),
    Sub(RType),
    Sll(RType),
    Slt(RType),
    Sltu(RType),
    Xor(RType),
    Srl(RType),
    Sra(RType),
    Or(RType),
    And(RType),

    // ===== RV32I: I-type ALU =====
    Addi(IType),
    Slti(IType),
    Sltiu(IType),
    Xori(IType),
    Ori(IType),
    Andi(IType),
    Slli(ShiftType),
    Srli(ShiftType),
    Srai(ShiftType),

    // ===== RV32I: Load =====
    Lb(IType),
    Lh(IType),
    Lw(IType),
    Lbu(IType),
    Lhu(IType),

    // ===== RV32I: Store =====
    Sb(SType),
    Sh(SType),
    Sw(SType),

    // ===== RV32I: Branch =====
    Beq(BType),
    Bne(BType),
    Blt(BType),
    Bge(BType),
    Bltu(BType),
    Bgeu(BType),

    // ===== RV32I: Upper Immediate =====
    Lui(UType),
    Auipc(UType),

    // ===== RV32I: Jump =====
    Jal(JType),
    Jalr(IType),

    // ===== RV32I: System =====
    Ecall,
    Ebreak,
    Fence,

    // ===== Zicsr: CSR 操作 =====
    Csrrw(CsrType),
    Csrrs(CsrType),
    Csrrc(CsrType),
    Csrrwi(CsrType),
    Csrrsi(CsrType),
    Csrrci(CsrType),

    // ===== 特权指令 =====
    Mret,
    Sret,
    Wfi,
    SfenceVma { rs1: usize, rs2: usize },
}
