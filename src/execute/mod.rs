mod rv32i;
mod zicsr;

use crate::cpu::Hart;
use crate::instruction::Instruction;
use crate::memory::Memory;

/// 执行一条已解码的指令
pub fn execute(hart: &mut Hart, mem: &mut Memory, inst: Instruction) {
    match inst {
        // RV32I
        Instruction::Add(_) | Instruction::Sub(_) | Instruction::Sll(_) |
        Instruction::Slt(_) | Instruction::Sltu(_) | Instruction::Xor(_) |
        Instruction::Srl(_) | Instruction::Sra(_) | Instruction::Or(_) |
        Instruction::And(_) |
        Instruction::Addi(_) | Instruction::Slti(_) | Instruction::Sltiu(_) |
        Instruction::Xori(_) | Instruction::Ori(_) | Instruction::Andi(_) |
        Instruction::Slli(_) | Instruction::Srli(_) | Instruction::Srai(_) |
        Instruction::Lb(_) | Instruction::Lh(_) | Instruction::Lw(_) |
        Instruction::Lbu(_) | Instruction::Lhu(_) |
        Instruction::Sb(_) | Instruction::Sh(_) | Instruction::Sw(_) |
        Instruction::Beq(_) | Instruction::Bne(_) | Instruction::Blt(_) |
        Instruction::Bge(_) | Instruction::Bltu(_) | Instruction::Bgeu(_) |
        Instruction::Lui(_) | Instruction::Auipc(_) |
        Instruction::Jal(_) | Instruction::Jalr(_) |
        Instruction::Ecall | Instruction::Ebreak | Instruction::Fence
            => rv32i::execute(hart, mem, inst),

        // Zicsr
        Instruction::Csrrw(_) | Instruction::Csrrs(_) | Instruction::Csrrc(_) |
        Instruction::Csrrwi(_) | Instruction::Csrrsi(_) | Instruction::Csrrci(_)
            => zicsr::execute(hart, inst),

        // 特权指令
        Instruction::Mret => hart.mret(),
        Instruction::Sret => { hart.pc += 4; } // TODO: 完整实现
        Instruction::Wfi  => { hart.pc += 4; } // NOP
        Instruction::SfenceVma { .. } => { hart.pc += 4; } // NOP（无 TLB）
    }
}
