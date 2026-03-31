mod rv32i;

use crate::cpu::Hart;
use crate::instruction::Instruction;
use crate::memory::Memory;

/// 执行一条已解码的指令
pub fn execute(hart: &mut Hart, mem: &mut Memory, inst: Instruction) {
    match inst {
        // RV32I 全部交给 rv32i 模块
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

        // M2 时在这里加:
        // Instruction::Mul(_) | ... => rv32m::execute(hart, mem, inst),
        // Instruction::LrW(_) | ... => rv32a::execute(hart, mem, inst),
    }
}
