use crate::cpu::Hart;
use crate::instruction::*;
use crate::memory::Memory;

pub fn execute(hart: &mut Hart, mem: &mut Memory, inst: Instruction) {
    match inst {
        // ===== R-type 算术 =====
        Instruction::Add(r)  => { hart.write_reg(r.rd, hart.read_reg(r.rs1).wrapping_add(hart.read_reg(r.rs2))); hart.pc += 4; }
        Instruction::Sub(r)  => { hart.write_reg(r.rd, hart.read_reg(r.rs1).wrapping_sub(hart.read_reg(r.rs2))); hart.pc += 4; }
        Instruction::Sll(r)  => { hart.write_reg(r.rd, hart.read_reg(r.rs1) << (hart.read_reg(r.rs2) & 0x1F)); hart.pc += 4; }
        Instruction::Slt(r)  => { hart.write_reg(r.rd, ((hart.read_reg(r.rs1) as i32) < (hart.read_reg(r.rs2) as i32)) as u32); hart.pc += 4; }
        Instruction::Sltu(r) => { hart.write_reg(r.rd, (hart.read_reg(r.rs1) < hart.read_reg(r.rs2)) as u32); hart.pc += 4; }
        Instruction::Xor(r)  => { hart.write_reg(r.rd, hart.read_reg(r.rs1) ^ hart.read_reg(r.rs2)); hart.pc += 4; }
        Instruction::Srl(r)  => { hart.write_reg(r.rd, hart.read_reg(r.rs1) >> (hart.read_reg(r.rs2) & 0x1F)); hart.pc += 4; }
        Instruction::Sra(r)  => { hart.write_reg(r.rd, ((hart.read_reg(r.rs1) as i32) >> (hart.read_reg(r.rs2) & 0x1F)) as u32); hart.pc += 4; }
        Instruction::Or(r)   => { hart.write_reg(r.rd, hart.read_reg(r.rs1) | hart.read_reg(r.rs2)); hart.pc += 4; }
        Instruction::And(r)  => { hart.write_reg(r.rd, hart.read_reg(r.rs1) & hart.read_reg(r.rs2)); hart.pc += 4; }

        // ===== I-type ALU =====
        Instruction::Addi(i)  => { hart.write_reg(i.rd, (hart.read_reg(i.rs1) as i32).wrapping_add(i.imm) as u32); hart.pc += 4; }
        Instruction::Slti(i)  => { hart.write_reg(i.rd, ((hart.read_reg(i.rs1) as i32) < i.imm) as u32); hart.pc += 4; }
        Instruction::Sltiu(i) => { hart.write_reg(i.rd, (hart.read_reg(i.rs1) < (i.imm as u32)) as u32); hart.pc += 4; }
        Instruction::Xori(i)  => { hart.write_reg(i.rd, hart.read_reg(i.rs1) ^ (i.imm as u32)); hart.pc += 4; }
        Instruction::Ori(i)   => { hart.write_reg(i.rd, hart.read_reg(i.rs1) | (i.imm as u32)); hart.pc += 4; }
        Instruction::Andi(i)  => { hart.write_reg(i.rd, hart.read_reg(i.rs1) & (i.imm as u32)); hart.pc += 4; }
        Instruction::Slli(s)  => { hart.write_reg(s.rd, hart.read_reg(s.rs1) << s.shamt); hart.pc += 4; }
        Instruction::Srli(s)  => { hart.write_reg(s.rd, hart.read_reg(s.rs1) >> s.shamt); hart.pc += 4; }
        Instruction::Srai(s)  => { hart.write_reg(s.rd, ((hart.read_reg(s.rs1) as i32) >> s.shamt) as u32); hart.pc += 4; }

        // ===== Load =====
        Instruction::Lb(i)  => { let addr = (hart.read_reg(i.rs1) as i32).wrapping_add(i.imm) as u32; hart.write_reg(i.rd, mem.read8(addr) as i8 as i32 as u32); hart.pc += 4; }
        Instruction::Lh(i)  => { let addr = (hart.read_reg(i.rs1) as i32).wrapping_add(i.imm) as u32; hart.write_reg(i.rd, mem.read16(addr) as i16 as i32 as u32); hart.pc += 4; }
        Instruction::Lw(i)  => { let addr = (hart.read_reg(i.rs1) as i32).wrapping_add(i.imm) as u32; hart.write_reg(i.rd, mem.read32(addr)); hart.pc += 4; }
        Instruction::Lbu(i) => { let addr = (hart.read_reg(i.rs1) as i32).wrapping_add(i.imm) as u32; hart.write_reg(i.rd, mem.read8(addr) as u32); hart.pc += 4; }
        Instruction::Lhu(i) => { let addr = (hart.read_reg(i.rs1) as i32).wrapping_add(i.imm) as u32; hart.write_reg(i.rd, mem.read16(addr) as u32); hart.pc += 4; }

        // ===== Store（写入时检查 tohost）=====
        Instruction::Sb(s) => { let addr = (hart.read_reg(s.rs1) as i32).wrapping_add(s.imm) as u32; mem.write8(addr, hart.read_reg(s.rs2) as u8); hart.pc += 4; }
        Instruction::Sh(s) => { let addr = (hart.read_reg(s.rs1) as i32).wrapping_add(s.imm) as u32; mem.write16(addr, hart.read_reg(s.rs2) as u16); hart.pc += 4; }
        Instruction::Sw(s) => {
            let addr = (hart.read_reg(s.rs1) as i32).wrapping_add(s.imm) as u32;
            let val = hart.read_reg(s.rs2);
            mem.write32(addr, val);
            // tohost 检测
            if let Some(tohost) = hart.tohost_addr {
                if addr == tohost && val != 0 {
                    hart.tohost_value = Some(val);
                }
            }
            hart.pc += 4;
        }

        // ===== Branch =====
        Instruction::Beq(b)  => branch(hart, hart.read_reg(b.rs1) == hart.read_reg(b.rs2), b.imm),
        Instruction::Bne(b)  => branch(hart, hart.read_reg(b.rs1) != hart.read_reg(b.rs2), b.imm),
        Instruction::Blt(b)  => branch(hart, (hart.read_reg(b.rs1) as i32) < (hart.read_reg(b.rs2) as i32), b.imm),
        Instruction::Bge(b)  => branch(hart, (hart.read_reg(b.rs1) as i32) >= (hart.read_reg(b.rs2) as i32), b.imm),
        Instruction::Bltu(b) => branch(hart, hart.read_reg(b.rs1) < hart.read_reg(b.rs2), b.imm),
        Instruction::Bgeu(b) => branch(hart, hart.read_reg(b.rs1) >= hart.read_reg(b.rs2), b.imm),

        // ===== Upper Immediate =====
        Instruction::Lui(u)   => { hart.write_reg(u.rd, u.imm); hart.pc += 4; }
        Instruction::Auipc(u) => { hart.write_reg(u.rd, hart.pc.wrapping_add(u.imm)); hart.pc += 4; }

        // ===== Jump =====
        Instruction::Jal(j) => {
            hart.write_reg(j.rd, hart.pc + 4);
            hart.pc = (hart.pc as i32).wrapping_add(j.imm) as u32;
        }
        Instruction::Jalr(i) => {
            let ret = hart.pc + 4;
            hart.pc = ((hart.read_reg(i.rs1) as i32).wrapping_add(i.imm) as u32) & !1;
            hart.write_reg(i.rd, ret);
        }

        // ===== System =====
        Instruction::Ecall => {
            use crate::cpu::CAUSE_ECALL_M;
            hart.trap(CAUSE_ECALL_M, 0);
        }
        Instruction::Ebreak => {
            hart.trap(3, hart.pc); // cause=3 (Breakpoint)
        }
        Instruction::Fence => { hart.pc += 4; }

        #[allow(unreachable_patterns)]
        _ => unreachable!("Non-RV32I instruction in rv32i::execute"),
    }
}

/// 分支辅助：条件成立则 PC += imm，否则 PC += 4
fn branch(hart: &mut Hart, taken: bool, imm: i32) {
    if taken {
        hart.pc = (hart.pc as i32).wrapping_add(imm) as u32;
    } else {
        hart.pc += 4;
    }
}
