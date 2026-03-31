use crate::instruction::*;

/// 将 32 位原始指令解码为 Instruction 枚举
pub fn decode(inst: u32) -> Instruction {
    let op = inst & 0x7F;
    match op {
        0b0110011 => decode_r_type(inst),
        0b0010011 => decode_i_type_alu(inst),
        0b0000011 => decode_load(inst),
        0b0100011 => decode_store(inst),
        0b1100011 => decode_branch(inst),
        0b0110111 => Instruction::Lui(UType { rd: rd(inst), imm: imm_u(inst) }),
        0b0010111 => Instruction::Auipc(UType { rd: rd(inst), imm: imm_u(inst) }),
        0b1101111 => Instruction::Jal(JType { rd: rd(inst), imm: imm_j(inst) }),
        0b1100111 => Instruction::Jalr(IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) }),
        0b1110011 => decode_system(inst),
        0b0001111 => Instruction::Fence,
        _ => panic!("Unknown opcode: 0b{:07b} (inst=0x{:08x})", op, inst),
    }
}

// ===== 字段提取辅助函数 =====

fn rd(inst: u32) -> usize { ((inst >> 7) & 0x1F) as usize }
fn funct3(inst: u32) -> u32 { (inst >> 12) & 0x7 }
fn rs1(inst: u32) -> usize { ((inst >> 15) & 0x1F) as usize }
fn rs2(inst: u32) -> usize { ((inst >> 20) & 0x1F) as usize }
fn funct7(inst: u32) -> u32 { (inst >> 25) & 0x7F }

fn imm_i(inst: u32) -> i32 { (inst as i32) >> 20 }

fn imm_s(inst: u32) -> i32 {
    let lo = (inst >> 7) & 0x1F;
    let hi = (inst >> 25) & 0x7F;
    (((hi << 5) | lo) as i32) << 20 >> 20
}

fn imm_b(inst: u32) -> i32 {
    let b11 = (inst >> 7) & 0x1;
    let b4_1 = (inst >> 8) & 0xF;
    let b10_5 = (inst >> 25) & 0x3F;
    let b12 = (inst >> 31) & 0x1;
    let imm = (b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1);
    ((imm as i32) << 19) >> 19
}

fn imm_u(inst: u32) -> u32 { inst & 0xFFFFF000 }

fn imm_j(inst: u32) -> i32 {
    let b19_12 = (inst >> 12) & 0xFF;
    let b11 = (inst >> 20) & 0x1;
    let b10_1 = (inst >> 21) & 0x3FF;
    let b20 = (inst >> 31) & 0x1;
    let imm = (b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1);
    ((imm as i32) << 11) >> 11
}

// ===== 分组解码 =====

fn decode_r_type(inst: u32) -> Instruction {
    let r = RType { rd: rd(inst), rs1: rs1(inst), rs2: rs2(inst) };
    let f3 = funct3(inst);
    let f7 = funct7(inst);
    match (f3, f7) {
        (0x0, 0x00) => Instruction::Add(r),
        (0x0, 0x20) => Instruction::Sub(r),
        (0x1, 0x00) => Instruction::Sll(r),
        (0x2, 0x00) => Instruction::Slt(r),
        (0x3, 0x00) => Instruction::Sltu(r),
        (0x4, 0x00) => Instruction::Xor(r),
        (0x5, 0x00) => Instruction::Srl(r),
        (0x5, 0x20) => Instruction::Sra(r),
        (0x6, 0x00) => Instruction::Or(r),
        (0x7, 0x00) => Instruction::And(r),
        _ => panic!("Unknown R-type: f3=0x{:x}, f7=0x{:x} (inst=0x{:08x})", f3, f7, inst),
    }
}

fn decode_i_type_alu(inst: u32) -> Instruction {
    let f3 = funct3(inst);
    match f3 {
        0x0 => Instruction::Addi(IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) }),
        0x2 => Instruction::Slti(IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) }),
        0x3 => Instruction::Sltiu(IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) }),
        0x4 => Instruction::Xori(IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) }),
        0x6 => Instruction::Ori(IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) }),
        0x7 => Instruction::Andi(IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) }),
        0x1 => Instruction::Slli(ShiftType { rd: rd(inst), rs1: rs1(inst), shamt: (inst >> 20) & 0x1F }),
        0x5 => {
            let shamt = (inst >> 20) & 0x1F;
            if funct7(inst) == 0x00 {
                Instruction::Srli(ShiftType { rd: rd(inst), rs1: rs1(inst), shamt })
            } else {
                Instruction::Srai(ShiftType { rd: rd(inst), rs1: rs1(inst), shamt })
            }
        }
        _ => unreachable!(),
    }
}

fn decode_load(inst: u32) -> Instruction {
    let i = IType { rd: rd(inst), rs1: rs1(inst), imm: imm_i(inst) };
    match funct3(inst) {
        0x0 => Instruction::Lb(i),
        0x1 => Instruction::Lh(i),
        0x2 => Instruction::Lw(i),
        0x4 => Instruction::Lbu(i),
        0x5 => Instruction::Lhu(i),
        f3 => panic!("Unknown load funct3: 0x{:x} (inst=0x{:08x})", f3, inst),
    }
}

fn decode_store(inst: u32) -> Instruction {
    let s = SType { rs1: rs1(inst), rs2: rs2(inst), imm: imm_s(inst) };
    match funct3(inst) {
        0x0 => Instruction::Sb(s),
        0x1 => Instruction::Sh(s),
        0x2 => Instruction::Sw(s),
        f3 => panic!("Unknown store funct3: 0x{:x} (inst=0x{:08x})", f3, inst),
    }
}

fn decode_branch(inst: u32) -> Instruction {
    let b = BType { rs1: rs1(inst), rs2: rs2(inst), imm: imm_b(inst) };
    match funct3(inst) {
        0x0 => Instruction::Beq(b),
        0x1 => Instruction::Bne(b),
        0x4 => Instruction::Blt(b),
        0x5 => Instruction::Bge(b),
        0x6 => Instruction::Bltu(b),
        0x7 => Instruction::Bgeu(b),
        f3 => panic!("Unknown branch funct3: 0x{:x} (inst=0x{:08x})", f3, inst),
    }
}

fn decode_system(inst: u32) -> Instruction {
    let f3 = funct3(inst);
    if f3 == 0 {
        // ECALL / EBREAK / MRET / SRET / WFI / SFENCE.VMA
        let funct7_val = funct7(inst);
        let rs2_val = rs2(inst);
        match (funct7_val, rs2_val) {
            (0x00, 0) => Instruction::Ecall,
            (0x00, 1) => Instruction::Ebreak,
            (0x18, 2) => Instruction::Mret,           // funct7=0011000, rs2=00010
            (0x08, 2) => Instruction::Sret,            // funct7=0001000, rs2=00010
            (0x08, 5) => Instruction::Wfi,             // funct7=0001000, rs2=00101
            (0x09, _) => Instruction::SfenceVma {      // funct7=0001001
                rs1: rs1(inst), rs2: rs2_val,
            },
            _ => panic!("Unknown SYSTEM funct7=0x{:02x}, rs2={} (inst=0x{:08x})", funct7_val, rs2_val, inst),
        }
    } else {
        // CSR 指令: funct3 = 001-011 (reg), 101-111 (imm)
        let csr_addr = ((inst >> 20) & 0xFFF) as u16;
        let c = CsrType { rd: rd(inst), rs1: rs1(inst), csr: csr_addr };
        match f3 {
            0x1 => Instruction::Csrrw(c),
            0x2 => Instruction::Csrrs(c),
            0x3 => Instruction::Csrrc(c),
            0x5 => Instruction::Csrrwi(c),
            0x6 => Instruction::Csrrsi(c),
            0x7 => Instruction::Csrrci(c),
            _ => panic!("Unknown CSR funct3=0x{:x} (inst=0x{:08x})", f3, inst),
        }
    }
}
