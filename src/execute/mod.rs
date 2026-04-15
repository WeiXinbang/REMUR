use crate::bus::Bus;
use crate::cpu::{Hart, AccessType, CAUSE_ECALL_U, CAUSE_ECALL_S, CAUSE_ECALL_M,
                  CAUSE_ILLEGAL_INST, CAUSE_INST_MISALIGNED,
                  MSTATUS, MSTATUS_TVM, MSTATUS_TSR, MSTATUS_TW,
                  SATP, MCOUNTEREN, SCOUNTEREN,
                  CYCLE, CYCLEH, INSTRET, INSTRETH};
use crate::instruction::*;

/// 执行一条已解码的指令
pub fn execute(hart: &mut Hart, bus: &mut Bus, inst: Instruction) {
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
                // RV32M
                ROp::Mul    => v1.wrapping_mul(v2),
                ROp::Mulh   => (((v1 as i32 as i64).wrapping_mul(v2 as i32 as i64)) >> 32) as u32,
                ROp::Mulhsu => (((v1 as i32 as i64).wrapping_mul(v2 as u64 as i64)) >> 32) as u32,
                ROp::Mulhu  => (((v1 as u64).wrapping_mul(v2 as u64)) >> 32) as u32,
                ROp::Div    => {
                    let a = v1 as i32; let b = v2 as i32;
                    if b == 0 { u32::MAX }
                    else if a == i32::MIN && b == -1 { a as u32 }
                    else { a.wrapping_div(b) as u32 }
                }
                ROp::Divu => if v2 == 0 { u32::MAX } else { v1 / v2 },
                ROp::Rem  => {
                    let a = v1 as i32; let b = v2 as i32;
                    if b == 0 { v1 }
                    else if a == i32::MIN && b == -1 { 0 }
                    else { a.wrapping_rem(b) as u32 }
                }
                ROp::Remu => if v2 == 0 { v1 } else { v1 % v2 },
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
            let pa = match hart.translate(bus, addr, AccessType::Read) {
                Ok(pa) => pa,
                Err((cause, tval)) => { hart.trap(cause, tval); return; }
            };
            let val = match op {
                LoadOp::Lb  => bus.read8(pa) as i8 as i32 as u32,
                LoadOp::Lh  => bus.read16(pa) as i16 as i32 as u32,
                LoadOp::Lw  => bus.read32(pa),
                LoadOp::Lbu => bus.read8(pa) as u32,
                LoadOp::Lhu => bus.read16(pa) as u32,
            };
            hart.write_reg(rd, val);
            hart.pc += 4;
        }

        // ===== Store =====
        Instruction::Store { op, rs1, rs2, imm } => {
            let addr = (hart.read_reg(rs1) as i32).wrapping_add(imm) as u32;
            let pa = match hart.translate(bus, addr, AccessType::Write) {
                Ok(pa) => pa,
                Err((cause, tval)) => { hart.trap(cause, tval); return; }
            };
            let val = hart.read_reg(rs2);
            match op {
                StoreOp::Sb => bus.write8(pa, val as u8),
                StoreOp::Sh => bus.write16(pa, val as u16),
                StoreOp::Sw => bus.write32(pa, val),
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
                let target = (hart.pc as i32).wrapping_add(imm) as u32;
                if target & 0x3 != 0 {
                    hart.trap(CAUSE_INST_MISALIGNED, target);
                } else {
                    hart.pc = target;
                }
            } else {
                hart.pc += 4;
            }
        }

        // ===== Upper Immediate =====
        Instruction::Lui   { rd, imm } => { hart.write_reg(rd, imm); hart.pc += 4; }
        Instruction::Auipc { rd, imm } => { hart.write_reg(rd, hart.pc.wrapping_add(imm)); hart.pc += 4; }

        // ===== Jump =====
        Instruction::Jal { rd, imm } => {
            let target = (hart.pc as i32).wrapping_add(imm) as u32;
            if target & 0x3 != 0 {
                hart.trap(CAUSE_INST_MISALIGNED, target);
            } else {
                hart.write_reg(rd, hart.pc + 4);
                hart.pc = target;
            }
        }
        Instruction::Jalr { rd, rs1, imm } => {
            let target = ((hart.read_reg(rs1) as i32).wrapping_add(imm) as u32) & !1;
            if target & 0x3 != 0 {
                hart.trap(CAUSE_INST_MISALIGNED, target);
            } else {
                let ret = hart.pc + 4;
                hart.pc = target;
                hart.write_reg(rd, ret);
            }
        }

        // ===== CSR =====
        Instruction::Csr { op, rd, rs1, csr } => {
            // CSR 访问控制：检查特权级和读写权限
            let csr_priv = (csr >> 8) & 0x3;
            let read_only = ((csr >> 10) & 0x3) == 0x3;
            let is_write = match op {
                CsrOp::Rw | CsrOp::Rwi => true,
                _ => rs1 != 0,
            };
            if hart.privilege < (csr_priv as u8) || (read_only && is_write) {
                hart.trap(CAUSE_ILLEGAL_INST, 0);
                return;
            }
            // TVM=1 时 S-mode 访问 satp 为非法
            if csr == SATP && hart.privilege == 1
                && (hart.read_csr(MSTATUS) & MSTATUS_TVM) != 0
            {
                hart.trap(CAUSE_ILLEGAL_INST, 0);
                return;
            }
            // mcounteren/scounteren 控制计数器访问
            if matches!(csr, CYCLE | CYCLEH | INSTRET | INSTRETH) && hart.privilege < 3 {
                let bit = if matches!(csr, CYCLE | CYCLEH) { 1u32 << 0 } else { 1u32 << 2 };
                if hart.read_csr(MCOUNTEREN) & bit == 0 {
                    hart.trap(CAUSE_ILLEGAL_INST, 0);
                    return;
                }
                if hart.privilege == 0 && hart.read_csr(SCOUNTEREN) & bit == 0 {
                    hart.trap(CAUSE_ILLEGAL_INST, 0);
                    return;
                }
            }
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

        // ===== RV32A (Atomic) =====
        Instruction::Amo { op, rd, rs1, rs2, .. } => {
            let addr = hart.read_reg(rs1);
            let access = if matches!(op, AmoOp::Lr) { AccessType::Read } else { AccessType::Write };
            let pa = match hart.translate(bus, addr, access) {
                Ok(pa) => pa,
                Err((cause, tval)) => { hart.trap(cause, tval); return; }
            };
            match op {
                AmoOp::Lr => {
                    let val = bus.read32(pa);
                    hart.write_reg(rd, val);
                    hart.reservation = Some(pa);
                }
                AmoOp::Sc => {
                    if hart.reservation == Some(pa) {
                        bus.write32(pa, hart.read_reg(rs2));
                        hart.write_reg(rd, 0); // success
                    } else {
                        hart.write_reg(rd, 1); // failure
                    }
                    hart.reservation = None;
                }
                _ => {
                    let old = bus.read32(pa);
                    let src = hart.read_reg(rs2);
                    let result = match op {
                        AmoOp::Swap => src,
                        AmoOp::Add  => old.wrapping_add(src),
                        AmoOp::Xor  => old ^ src,
                        AmoOp::And  => old & src,
                        AmoOp::Or   => old | src,
                        AmoOp::Min  => (old as i32).min(src as i32) as u32,
                        AmoOp::Max  => (old as i32).max(src as i32) as u32,
                        AmoOp::Minu => old.min(src),
                        AmoOp::Maxu => old.max(src),
                        AmoOp::Lr | AmoOp::Sc => unreachable!(),
                    };
                    bus.write32(pa, result);
                    hart.write_reg(rd, old);
                }
            }
            hart.pc += 4;
        }

        // ===== System / Privilege =====
        Instruction::Ecall  => {
            let cause = match hart.privilege {
                0 => CAUSE_ECALL_U,
                1 => CAUSE_ECALL_S,
                _ => CAUSE_ECALL_M,
            };
            hart.trap(cause, 0);
        }
        Instruction::Ebreak => hart.trap(3, hart.pc),
        Instruction::Fence  => { hart.pc += 4; }
        Instruction::Mret   => {
            if hart.privilege < 3 { hart.trap(CAUSE_ILLEGAL_INST, 0); return; }
            hart.mret();
        }
        Instruction::Sret   => {
            if hart.privilege < 1 { hart.trap(CAUSE_ILLEGAL_INST, 0); return; }
            // TSR=1 时 S-mode 执行 SRET 为非法
            if hart.privilege == 1 && (hart.read_csr(MSTATUS) & MSTATUS_TSR) != 0 {
                hart.trap(CAUSE_ILLEGAL_INST, 0); return;
            }
            hart.sret();
        }
        Instruction::Wfi    => {
            // TW=1 时非 M-mode 执行 WFI 为非法
            if hart.privilege < 3 && (hart.read_csr(MSTATUS) & MSTATUS_TW) != 0 {
                hart.trap(CAUSE_ILLEGAL_INST, 0); return;
            }
            hart.pc += 4;
        }
        Instruction::SfenceVma { .. } => {
            // TVM=1 时 S-mode 执行 SFENCE.VMA 为非法
            if hart.privilege == 1 && (hart.read_csr(MSTATUS) & MSTATUS_TVM) != 0 {
                hart.trap(CAUSE_ILLEGAL_INST, 0); return;
            }
            #[cfg(feature = "cached-decode")]
            hart.flush_decode_cache();
            hart.pc += 4;
        }

        // ===== Illegal instruction =====
        Instruction::Illegal(inst_val) => {
            hart.trap(CAUSE_ILLEGAL_INST, inst_val);
        }
    }
}
