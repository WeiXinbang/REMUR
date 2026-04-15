use crate::instruction::*;

/// 将 32 位原始指令解码为 Instruction 枚举
pub fn decode(inst: u32) -> Instruction {
    let op = inst & 0x7F;
    match op {
        0b0110011 => decode_r(inst),
        0b0010011 => decode_i_alu(inst),
        0b0000011 => decode_load(inst),
        0b0100011 => decode_store(inst),
        0b1100011 => decode_branch(inst),
        0b0101111 => decode_amo(inst),
        0b0110111 => Instruction::Lui   { rd: rd(inst), imm: imm_u(inst) },
        0b0010111 => Instruction::Auipc { rd: rd(inst), imm: imm_u(inst) },
        0b1101111 => Instruction::Jal   { rd: rd(inst), imm: imm_j(inst) },
        0b1100111 => Instruction::Jalr  { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) },
        0b1110011 => decode_system(inst),
        0b0001111 => Instruction::Fence,
        _ => Instruction::Illegal(inst),
    }
}

// ===== 字段提取 =====

fn rd(inst: u32) -> usize    { ((inst >> 7)  & 0x1F) as usize }
fn rs1(inst: u32) -> usize   { ((inst >> 15) & 0x1F) as usize }
fn rs2(inst: u32) -> usize   { ((inst >> 20) & 0x1F) as usize }
fn funct3(inst: u32) -> u32  { (inst >> 12) & 0x7 }
fn funct7(inst: u32) -> u32  { (inst >> 25) & 0x7F }
fn imm_i(inst: u32) -> i32   { (inst as i32) >> 20 }
fn imm_u(inst: u32) -> u32   { inst & 0xFFFFF000 }

fn imm_s(inst: u32) -> i32 {
    let v = ((inst >> 25) << 5) | ((inst >> 7) & 0x1F);
    ((v as i32) << 20) >> 20
}

fn imm_b(inst: u32) -> i32 {
    let v = ((inst >> 31) << 12) | (((inst >> 7) & 1) << 11)
          | (((inst >> 25) & 0x3F) << 5) | (((inst >> 8) & 0xF) << 1);
    ((v as i32) << 19) >> 19
}

fn imm_j(inst: u32) -> i32 {
    let v = ((inst >> 31) << 20) | (((inst >> 12) & 0xFF) << 12)
          | (((inst >> 20) & 1) << 11) | (((inst >> 21) & 0x3FF) << 1);
    ((v as i32) << 11) >> 11
}

// ===== 分组解码 =====

fn decode_r(inst: u32) -> Instruction {
    let (rd, rs1, rs2) = (rd(inst), rs1(inst), rs2(inst));
    let op = match (funct3(inst), funct7(inst)) {
        (0x0, 0x00) => ROp::Add,
        (0x0, 0x20) => ROp::Sub,
        (0x1, 0x00) => ROp::Sll,
        (0x2, 0x00) => ROp::Slt,
        (0x3, 0x00) => ROp::Sltu,
        (0x4, 0x00) => ROp::Xor,
        (0x5, 0x00) => ROp::Srl,
        (0x5, 0x20) => ROp::Sra,
        (0x6, 0x00) => ROp::Or,
        (0x7, 0x00) => ROp::And,
        // RV32M (funct7 = 0x01)
        (0x0, 0x01) => ROp::Mul,
        (0x1, 0x01) => ROp::Mulh,
        (0x2, 0x01) => ROp::Mulhsu,
        (0x3, 0x01) => ROp::Mulhu,
        (0x4, 0x01) => ROp::Div,
        (0x5, 0x01) => ROp::Divu,
        (0x6, 0x01) => ROp::Rem,
        (0x7, 0x01) => ROp::Remu,
        (f3, f7) => return Instruction::Illegal(inst),
    };
    Instruction::R { op, rd, rs1, rs2 }
}

fn decode_i_alu(inst: u32) -> Instruction {
    let (rd, rs1) = (rd(inst), rs1(inst));
    match funct3(inst) {
        0x0 => Instruction::I { op: IOp::Addi,  rd, rs1, imm: imm_i(inst) },
        0x2 => Instruction::I { op: IOp::Slti,  rd, rs1, imm: imm_i(inst) },
        0x3 => Instruction::I { op: IOp::Sltiu, rd, rs1, imm: imm_i(inst) },
        0x4 => Instruction::I { op: IOp::Xori,  rd, rs1, imm: imm_i(inst) },
        0x6 => Instruction::I { op: IOp::Ori,   rd, rs1, imm: imm_i(inst) },
        0x7 => Instruction::I { op: IOp::Andi,  rd, rs1, imm: imm_i(inst) },
        0x1 => {
            if funct7(inst) != 0 { return Instruction::Illegal(inst); }
            Instruction::Shift { op: ShiftOp::Slli, rd, rs1, shamt: (inst >> 20) & 0x1F }
        }
        0x5 => {
            let shamt = (inst >> 20) & 0x1F;
            let op = match funct7(inst) {
                0x00 => ShiftOp::Srli,
                0x20 => ShiftOp::Srai,
                _ => return Instruction::Illegal(inst),
            };
            Instruction::Shift { op, rd, rs1, shamt }
        }
        _ => return Instruction::Illegal(inst),
    }
}

fn decode_load(inst: u32) -> Instruction {
    let (rd, rs1, imm) = (rd(inst), rs1(inst), imm_i(inst));
    let op = match funct3(inst) {
        0x0 => LoadOp::Lb, 0x1 => LoadOp::Lh, 0x2 => LoadOp::Lw,
        0x4 => LoadOp::Lbu, 0x5 => LoadOp::Lhu,
        f3 => return Instruction::Illegal(inst),
    };
    Instruction::Load { op, rd, rs1, imm }
}

fn decode_store(inst: u32) -> Instruction {
    let (rs1, rs2, imm) = (rs1(inst), rs2(inst), imm_s(inst));
    let op = match funct3(inst) {
        0x0 => StoreOp::Sb, 0x1 => StoreOp::Sh, 0x2 => StoreOp::Sw,
        f3 => return Instruction::Illegal(inst),
    };
    Instruction::Store { op, rs1, rs2, imm }
}

fn decode_branch(inst: u32) -> Instruction {
    let (rs1, rs2, imm) = (rs1(inst), rs2(inst), imm_b(inst));
    let op = match funct3(inst) {
        0x0 => BrOp::Beq,  0x1 => BrOp::Bne,
        0x4 => BrOp::Blt,  0x5 => BrOp::Bge,
        0x6 => BrOp::Bltu, 0x7 => BrOp::Bgeu,
        f3 => return Instruction::Illegal(inst),
    };
    Instruction::Branch { op, rs1, rs2, imm }
}

fn decode_amo(inst: u32) -> Instruction {
    let (rd, rs1, rs2) = (rd(inst), rs1(inst), rs2(inst));
    let funct5 = (inst >> 27) & 0x1F;
    let aq = ((inst >> 26) & 1) != 0;
    let rl = ((inst >> 25) & 1) != 0;
    let op = match funct5 {
        0x02 => AmoOp::Lr,
        0x03 => AmoOp::Sc,
        0x01 => AmoOp::Swap,
        0x00 => AmoOp::Add,
        0x04 => AmoOp::Xor,
        0x0C => AmoOp::And,
        0x08 => AmoOp::Or,
        0x10 => AmoOp::Min,
        0x14 => AmoOp::Max,
        0x18 => AmoOp::Minu,
        0x1C => AmoOp::Maxu,
        f5 => return Instruction::Illegal(inst),
    };
    Instruction::Amo { op, rd, rs1, rs2, aq, rl }
}

fn decode_system(inst: u32) -> Instruction {
    let f3 = funct3(inst);
    if f3 == 0 {
        match (funct7(inst), rs2(inst)) {
            (0x00, 0) => Instruction::Ecall,
            (0x00, 1) => Instruction::Ebreak,
            (0x18, 2) => Instruction::Mret,
            (0x08, 2) => Instruction::Sret,
            (0x08, 5) => Instruction::Wfi,
            (0x09, _) => Instruction::SfenceVma { rs1: rs1(inst), rs2: rs2(inst) },
            (f7, r2) => return Instruction::Illegal(inst),
        }
    } else {
        let (rd, rs1, csr) = (rd(inst), rs1(inst), ((inst >> 20) & 0xFFF) as u16);
        let op = match f3 {
            0x1 => CsrOp::Rw,  0x2 => CsrOp::Rs,  0x3 => CsrOp::Rc,
            0x5 => CsrOp::Rwi, 0x6 => CsrOp::Rsi, 0x7 => CsrOp::Rci,
            _ => return Instruction::Illegal(inst),
        };
        Instruction::Csr { op, rd, rs1, csr }
    }
}
