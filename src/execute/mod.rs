use crate::cpu::{Hart, CAUSE_ECALL_M};
use crate::instruction::*;
use crate::memory::Memory;

/// 执行一条已解码的指令
pub fn execute(hart: &mut Hart, mem: &mut Memory, inst: Instruction) {
    match inst {
        // ===== R-type =====
        Instruction::R { op, rd, rs1, rs2 } => {
            let v1 = hart.read_reg(rs1);
            let v2 = hart.read_reg(rs2);
            let result = match op {
                ROp::Add  => v1.wrapping_add(v2),
                ROp::Sub  => v1.wrapping_sub(v2),
                ROp::Sll  => v1 << (v2 & 0x1F),
                ROp::Slt  => ((v1 as i32) < (v2 as i32)) as u32,
                ROp::Sltu => (v1 < v2) as u32,
                ROp::Xor  => v1 ^ v2,
                ROp::Srl  => v1 >> (v2 & 0x1F),
                ROp::Sra  => ((v1 as i32) >> (v2 & 0x1F)) as u32,
                ROp::Or   => v1 | v2,
                ROp::And  => v1 & v2,
            };
            hart.write_reg(rd, result);
            hart.pc += 4;
        }

        // ===== I-type ALU =====
        Instruction::I { op, rd, rs1, imm } => {
            let v = hart.read_reg(rs1);
            let result = match op {
                IOp::Addi  => (v as i32).wrapping_add(imm) as u32,
                IOp::Slti  => ((v as i32) < imm) as u32,
                IOp::Sltiu => (v < (imm as u32)) as u32,
                IOp::Xori  => v ^ (imm as u32),
                IOp::Ori   => v | (imm as u32),
                IOp::Andi  => v & (imm as u32),
            };
            hart.write_reg(rd, result);
            hart.pc += 4;
        }

        // ===== Shift =====
        Instruction::Shift { op, rd, rs1, shamt } => {
            let v = hart.read_reg(rs1);
            let result = match op {
                ShiftOp::Slli => v << shamt,
                ShiftOp::Srli => v >> shamt,
                ShiftOp::Srai => ((v as i32) >> shamt) as u32,
            };
            hart.write_reg(rd, result);
            hart.pc += 4;
        }

        // ===== Load =====
        Instruction::Load { op, rd, rs1, imm } => {
            let addr = (hart.read_reg(rs1) as i32).wrapping_add(imm) as u32;
            let val = match op {
                LoadOp::Lb  => mem.read8(addr) as i8 as i32 as u32,
                LoadOp::Lh  => mem.read16(addr) as i16 as i32 as u32,
                LoadOp::Lw  => mem.read32(addr),
                LoadOp::Lbu => mem.read8(addr) as u32,
                LoadOp::Lhu => mem.read16(addr) as u32,
            };
            hart.write_reg(rd, val);
            hart.pc += 4;
        }

        // ===== Store =====
        Instruction::Store { op, rs1, rs2, imm } => {
            let addr = (hart.read_reg(rs1) as i32).wrapping_add(imm) as u32;
            let val = hart.read_reg(rs2);
            match op {
                StoreOp::Sb => mem.write8(addr, val as u8),
                StoreOp::Sh => mem.write16(addr, val as u16),
                StoreOp::Sw => {
                    mem.write32(addr, val);
                    if let Some(tohost) = hart.tohost_addr {
                        if addr == tohost && val != 0 {
                            hart.tohost_value = Some(val);
                        }
                    }
                }
            }
            hart.pc += 4;
        }

        // ===== Branch =====
        Instruction::Branch { op, rs1, rs2, imm } => {
            let v1 = hart.read_reg(rs1);
            let v2 = hart.read_reg(rs2);
            let taken = match op {
                BrOp::Beq  => v1 == v2,
                BrOp::Bne  => v1 != v2,
                BrOp::Blt  => (v1 as i32) < (v2 as i32),
                BrOp::Bge  => (v1 as i32) >= (v2 as i32),
                BrOp::Bltu => v1 < v2,
                BrOp::Bgeu => v1 >= v2,
            };
            if taken {
                hart.pc = (hart.pc as i32).wrapping_add(imm) as u32;
            } else {
                hart.pc += 4;
            }
        }

        // ===== Upper Immediate =====
        Instruction::Lui   { rd, imm } => { hart.write_reg(rd, imm); hart.pc += 4; }
        Instruction::Auipc { rd, imm } => { hart.write_reg(rd, hart.pc.wrapping_add(imm)); hart.pc += 4; }

        // ===== Jump =====
        Instruction::Jal { rd, imm } => {
            hart.write_reg(rd, hart.pc + 4);
            hart.pc = (hart.pc as i32).wrapping_add(imm) as u32;
        }
        Instruction::Jalr { rd, rs1, imm } => {
            let ret = hart.pc + 4;
            hart.pc = ((hart.read_reg(rs1) as i32).wrapping_add(imm) as u32) & !1;
            hart.write_reg(rd, ret);
        }

        // ===== CSR =====
        Instruction::Csr { op, rd, rs1, csr } => {
            let old = hart.read_csr(csr);
            match op {
                CsrOp::Rw  => { hart.write_csr(csr, hart.read_reg(rs1)); }
                CsrOp::Rs  => { if rs1 != 0 { hart.write_csr(csr, old | hart.read_reg(rs1)); } }
                CsrOp::Rc  => { if rs1 != 0 { hart.write_csr(csr, old & !hart.read_reg(rs1)); } }
                CsrOp::Rwi => { hart.write_csr(csr, rs1 as u32); }
                CsrOp::Rsi => { if rs1 != 0 { hart.write_csr(csr, old | rs1 as u32); } }
                CsrOp::Rci => { if rs1 != 0 { hart.write_csr(csr, old & !(rs1 as u32)); } }
            }
            hart.write_reg(rd, old);
            hart.pc += 4;
        }

        // ===== System / Privilege =====
        Instruction::Ecall  => hart.trap(CAUSE_ECALL_M, 0),
        Instruction::Ebreak => hart.trap(3, hart.pc),
        Instruction::Fence  => { hart.pc += 4; }
        Instruction::Mret   => hart.mret(),
        Instruction::Sret   => { hart.pc += 4; }
        Instruction::Wfi    => { hart.pc += 4; }
        Instruction::SfenceVma { .. } => { hart.pc += 4; }
    }
}
