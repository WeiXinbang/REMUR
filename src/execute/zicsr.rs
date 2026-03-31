use crate::cpu::Hart;
use crate::instruction::Instruction;

pub fn execute(hart: &mut Hart, inst: Instruction) {
    match inst {
        Instruction::Csrrw(c) => {
            let old = hart.read_csr(c.csr);
            hart.write_csr(c.csr, hart.read_reg(c.rs1));
            hart.write_reg(c.rd, old);
        }
        Instruction::Csrrs(c) => {
            let old = hart.read_csr(c.csr);
            if c.rs1 != 0 {
                hart.write_csr(c.csr, old | hart.read_reg(c.rs1));
            }
            hart.write_reg(c.rd, old);
        }
        Instruction::Csrrc(c) => {
            let old = hart.read_csr(c.csr);
            if c.rs1 != 0 {
                hart.write_csr(c.csr, old & !hart.read_reg(c.rs1));
            }
            hart.write_reg(c.rd, old);
        }
        Instruction::Csrrwi(c) => {
            let old = hart.read_csr(c.csr);
            hart.write_csr(c.csr, c.rs1 as u32); // rs1 字段当作 5 位立即数
            hart.write_reg(c.rd, old);
        }
        Instruction::Csrrsi(c) => {
            let old = hart.read_csr(c.csr);
            if c.rs1 != 0 {
                hart.write_csr(c.csr, old | (c.rs1 as u32));
            }
            hart.write_reg(c.rd, old);
        }
        Instruction::Csrrci(c) => {
            let old = hart.read_csr(c.csr);
            if c.rs1 != 0 {
                hart.write_csr(c.csr, old & !(c.rs1 as u32));
            }
            hart.write_reg(c.rd, old);
        }
        _ => unreachable!(),
    }
    hart.pc += 4;
}
