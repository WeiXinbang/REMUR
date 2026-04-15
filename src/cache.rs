use crate::instruction::Instruction;

/// 直接映射译码缓存：避免循环中重复 decode 同一条指令
/// 512 条目 ≈ 20KB，适合 L1 缓存
const DECODE_CACHE_SIZE: usize = 512;
const DECODE_CACHE_MASK: usize = DECODE_CACHE_SIZE - 1;

pub struct DecodeCache {
    /// (物理 PC, 已译码指令)。phys_pc=0 表示空槽
    entries: Box<[(u32, Instruction)]>,
}

impl DecodeCache {
    pub fn new() -> Self {
        Self {
            entries: vec![(0, Instruction::Illegal(0)); DECODE_CACHE_SIZE].into_boxed_slice(),
        }
    }

    #[inline(always)]
    fn index(phys_pc: u32) -> usize {
        (phys_pc as usize >> 2) & DECODE_CACHE_MASK
    }

    /// 查缓存：命中返回 Some(inst)，未命中返回 None
    #[inline(always)]
    pub fn lookup(&self, phys_pc: u32) -> Option<Instruction> {
        let idx = Self::index(phys_pc);
        let (cached_pc, inst) = self.entries[idx];
        if cached_pc == phys_pc {
            Some(inst)
        } else {
            None
        }
    }

    /// 插入缓存条目
    #[inline(always)]
    pub fn insert(&mut self, phys_pc: u32, inst: Instruction) {
        let idx = Self::index(phys_pc);
        self.entries[idx] = (phys_pc, inst);
    }

    /// 清空缓存（SFENCE.VMA 或 satp 写入时调用）
    pub fn flush(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = (0, Instruction::Illegal(0));
        }
    }
}
