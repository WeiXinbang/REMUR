use crate::memory::Memory;
use crate::decode;
use crate::execute;

/// RISC-V Hart（硬件线程）
pub struct Hart {
    pub regs: [u32; 32], // x0-x31 通用寄存器
    pub pc: u32,         // 程序计数器
}

impl Hart {
    pub fn new() -> Self {
        Hart {
            regs: [0; 32],
            pc: 0,
        }
    }

    /// 读寄存器（x0 永远返回 0）
    pub fn read_reg(&self, idx: usize) -> u32 {
        if idx == 0 { 0 } else { self.regs[idx] }
    }

    /// 写寄存器（x0 写入被忽略）
    pub fn write_reg(&mut self, idx: usize, val: u32) {
        if idx != 0 {
            self.regs[idx] = val;
        }
    }

    /// 取指 → 译码 → 执行
    pub fn step(&mut self, mem: &mut Memory) {
        let raw = mem.read32(self.pc);
        let inst = decode::decode(raw);
        execute::execute(self, mem, inst);
    }

    /// 运行指定周期数
    pub fn run(&mut self, mem: &mut Memory, max_cycles: u64) {
        for _ in 0..max_cycles {
            self.step(mem);
        }
    }
}
