use crate::memory::Memory;
use crate::decode;
use crate::execute;

// CSR 地址常量
pub const MSTATUS: u16   = 0x300;
pub const MISA: u16      = 0x301;
pub const MEDELEG: u16   = 0x302;
pub const MIDELEG: u16   = 0x303;
pub const MIE: u16       = 0x304;
pub const MTVEC: u16     = 0x305;
pub const MCOUNTEREN: u16 = 0x306;
pub const MSCRATCH: u16  = 0x340;
pub const MEPC: u16      = 0x341;
pub const MCAUSE: u16    = 0x342;
pub const MTVAL: u16     = 0x343;
pub const MIP: u16       = 0x344;
pub const PMPCFG0: u16   = 0x3A0;
pub const PMPADDR0: u16  = 0x3B0;
pub const MHARTID: u16   = 0xF14;
pub const MCYCLE: u16    = 0xB00;
pub const MINSTRET: u16  = 0xB02;

// mstatus 位域
pub const MSTATUS_MIE: u32  = 1 << 3;
pub const MSTATUS_MPIE: u32 = 1 << 7;
pub const MSTATUS_MPP_MASK: u32 = 0x3 << 11;
pub const MSTATUS_MPP_SHIFT: u32 = 11;
pub const MSTATUS_FS_MASK: u32 = 0x3 << 13;

// 异常原因
pub const CAUSE_ECALL_M: u32 = 11;

/// RISC-V Hart（硬件线程）
pub struct Hart {
    pub regs: [u32; 32],
    pub pc: u32,
    pub csrs: [u32; 4096], // CSR 寄存器（按地址索引）
    pub tohost_addr: Option<u32>,
    pub tohost_value: Option<u32>,
}

impl Hart {
    pub fn new() -> Self {
        let mut hart = Hart {
            regs: [0; 32],
            pc: 0,
            csrs: [0; 4096],
            tohost_addr: None,
            tohost_value: None,
        };
        // misa: RV32IMA (bits: I=8, M=12, A=0)
        hart.csrs[MISA as usize] = (1 << 30)  // MXL=1 (32-bit)
            | (1 << 8)   // I
            | (1 << 12)  // M
            | (1 << 0);  // A
        hart
    }

    pub fn read_reg(&self, idx: usize) -> u32 {
        if idx == 0 { 0 } else { self.regs[idx] }
    }

    pub fn write_reg(&mut self, idx: usize, val: u32) {
        if idx != 0 {
            self.regs[idx] = val;
        }
    }

    /// 读 CSR
    pub fn read_csr(&self, addr: u16) -> u32 {
        self.csrs[addr as usize]
    }

    /// 写 CSR
    pub fn write_csr(&mut self, addr: u16, val: u32) {
        match addr {
            MHARTID => {} // 只读，忽略写入
            MISA => {}    // 只读，忽略写入
            _ => { self.csrs[addr as usize] = val; }
        }
    }

    /// 触发 trap（异常/中断）
    pub fn trap(&mut self, cause: u32, tval: u32) {
        let mtvec = self.read_csr(MTVEC);
        // 保存当前状态
        self.write_csr(MEPC, self.pc);
        self.write_csr(MCAUSE, cause);
        self.write_csr(MTVAL, tval);

        // mstatus: 保存 MIE → MPIE，设置 MPP=M(3)，清除 MIE
        let mstatus = self.read_csr(MSTATUS);
        let mie_bit = (mstatus & MSTATUS_MIE) != 0;
        let new_mstatus = (mstatus & !MSTATUS_MIE & !MSTATUS_MPIE & !MSTATUS_MPP_MASK)
            | (if mie_bit { MSTATUS_MPIE } else { 0 })
            | (3 << MSTATUS_MPP_SHIFT); // MPP = M-mode
        self.write_csr(MSTATUS, new_mstatus);

        // 跳转到 trap handler
        let base = mtvec & !0x3;
        self.pc = base;
    }

    /// 执行 MRET
    pub fn mret(&mut self) {
        let mstatus = self.read_csr(MSTATUS);
        // 恢复 MIE ← MPIE
        let mpie = (mstatus & MSTATUS_MPIE) != 0;
        let new_mstatus = (mstatus & !MSTATUS_MIE & !MSTATUS_MPIE)
            | (if mpie { MSTATUS_MIE } else { 0 })
            | MSTATUS_MPIE; // MPIE 设为 1
        self.write_csr(MSTATUS, new_mstatus);

        // PC ← MEPC
        self.pc = self.read_csr(MEPC);
    }

    /// 取指 → 译码 → 执行
    pub fn step(&mut self, mem: &mut Memory) {
        let raw = mem.read32(self.pc);
        let inst = decode::decode(raw);
        execute::execute(self, mem, inst);
    }

    /// 运行指定周期数，返回 tohost 值（如果有）
    pub fn run(&mut self, mem: &mut Memory, max_cycles: u64) -> Option<u32> {
        for _ in 0..max_cycles {
            self.step(mem);
            if let Some(val) = self.tohost_value {
                return Some(val);
            }
        }
        None
    }
}
