#![allow(dead_code)]

use crate::bus::Bus;
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
pub const MCYCLEH: u16   = 0xB80;
pub const MINSTRETH: u16 = 0xB82;

// 只读影子计数器（U/S-mode 可访问，受 mcounteren 限制）
pub const CYCLE: u16     = 0xC00;
pub const INSTRET: u16   = 0xC02;
pub const CYCLEH: u16    = 0xC80;
pub const INSTRETH: u16  = 0xC82;

// S-mode CSR 地址
pub const SSTATUS: u16    = 0x100;
pub const SIE: u16        = 0x104;
pub const STVEC: u16      = 0x105;
pub const SCOUNTEREN: u16 = 0x106;
pub const SSCRATCH: u16   = 0x140;
pub const SEPC: u16       = 0x141;
pub const SCAUSE: u16     = 0x142;
pub const STVAL: u16      = 0x143;
pub const SIP: u16        = 0x144;
pub const SATP: u16       = 0x180;

// sstatus 可见位掩码（S-mode 只能看到这些 mstatus 位）
pub const SSTATUS_MASK: u32 = MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP
    | MSTATUS_FS_MASK | MSTATUS_SUM | MSTATUS_MXR;

// sie/sip 可见位掩码（S-mode 中断位：SSIE=1, STIE=5, SEIE=9）
pub const SIE_MASK: u32 = (1 << 1) | (1 << 5) | (1 << 9);

// mstatus 位域
pub const MSTATUS_SIE: u32   = 1 << 1;
pub const MSTATUS_MIE: u32   = 1 << 3;
pub const MSTATUS_SPIE: u32  = 1 << 5;
pub const MSTATUS_MPIE: u32  = 1 << 7;
pub const MSTATUS_SPP: u32   = 1 << 8;
pub const MSTATUS_MPP_MASK: u32 = 0x3 << 11;
pub const MSTATUS_MPP_SHIFT: u32 = 11;
pub const MSTATUS_FS_MASK: u32 = 0x3 << 13;
pub const MSTATUS_MPRV: u32  = 1 << 17;
pub const MSTATUS_SUM: u32   = 1 << 18;
pub const MSTATUS_MXR: u32   = 1 << 19;
pub const MSTATUS_TVM: u32   = 1 << 20;
pub const MSTATUS_TW: u32    = 1 << 21;
pub const MSTATUS_TSR: u32   = 1 << 22;

// 异常原因
pub const CAUSE_INST_MISALIGNED: u32   = 0;
pub const CAUSE_ILLEGAL_INST: u32      = 2;
pub const CAUSE_BREAKPOINT: u32        = 3;
pub const CAUSE_LOAD_MISALIGNED: u32   = 4;
pub const CAUSE_STORE_MISALIGNED: u32  = 6;
pub const CAUSE_ECALL_U: u32           = 8;
pub const CAUSE_ECALL_S: u32           = 9;
pub const CAUSE_ECALL_M: u32           = 11;
pub const CAUSE_INST_PAGE_FAULT: u32   = 12;
pub const CAUSE_LOAD_PAGE_FAULT: u32   = 13;
pub const CAUSE_STORE_PAGE_FAULT: u32  = 15;

// Sv32 页表项位域
const PTE_V: u32 = 1 << 0;
const PTE_R: u32 = 1 << 1;
const PTE_W: u32 = 1 << 2;
const PTE_X: u32 = 1 << 3;
const PTE_U: u32 = 1 << 4;
const PTE_A: u32 = 1 << 6;
const PTE_D: u32 = 1 << 7;

/// 内存访问类型（用于地址翻译）
pub enum AccessType { Execute, Read, Write }

/// RISC-V Hart（硬件线程）
pub struct Hart {
    pub regs: [u32; 32],
    pub pc: u32,
    pub csrs: [u32; 4096],
    pub privilege: u8,           // 当前特权级: 0=U, 1=S, 3=M
    pub reservation: Option<u32>, // LR/SC 保留地址
    suppress_instret: bool,      // 写 minstret/minstreth 后抑制本次递增
}

impl Hart {
    pub fn new() -> Self {
        let mut hart = Hart {
            regs: [0; 32],
            pc: 0,
            csrs: [0; 4096],
            privilege: 3, // 复位为 M-mode
            reservation: None,
            suppress_instret: false,
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

    /// 读 CSR（处理 S-mode 别名）
    pub fn read_csr(&self, addr: u16) -> u32 {
        match addr {
            SSTATUS => self.csrs[MSTATUS as usize] & SSTATUS_MASK,
            SIE => self.csrs[MIE as usize] & SIE_MASK,
            SIP => self.csrs[MIP as usize] & SIE_MASK,
            // 只读影子计数器 → 读对应的 M-mode 计数器
            CYCLE    => self.csrs[MCYCLE as usize],
            INSTRET  => self.csrs[MINSTRET as usize],
            CYCLEH   => self.csrs[MCYCLEH as usize],
            INSTRETH => self.csrs[MINSTRETH as usize],
            _ => self.csrs[addr as usize],
        }
    }

    /// 写 CSR（处理只读和 S-mode 别名）
    pub fn write_csr(&mut self, addr: u16, val: u32) {
        match addr {
            MHARTID | MISA => {} // 只读
            MTVEC | STVEC => { self.csrs[addr as usize] = val & !0x3; } // 只支持 Direct 模式
            SSTATUS => {
                let mstatus = self.csrs[MSTATUS as usize];
                self.csrs[MSTATUS as usize] = (mstatus & !SSTATUS_MASK) | (val & SSTATUS_MASK);
            }
            SIE => {
                let mie = self.csrs[MIE as usize];
                self.csrs[MIE as usize] = (mie & !SIE_MASK) | (val & SIE_MASK);
            }
            SIP => {
                let mip = self.csrs[MIP as usize];
                self.csrs[MIP as usize] = (mip & !SIE_MASK) | (val & SIE_MASK);
            }
            MINSTRET | MINSTRETH => {
                self.csrs[addr as usize] = val;
                self.suppress_instret = true;
            }
            _ => { self.csrs[addr as usize] = val; }
        }
    }

    /// 触发 trap：根据 medeleg 决定 trap 到 M-mode 还是 S-mode
    pub fn trap(&mut self, cause: u32, tval: u32) {
        let is_interrupt = (cause & 0x8000_0000) != 0;
        let cause_code = cause & 0x7FFF_FFFF;

        // 检查是否委托到 S-mode
        let deleg = if is_interrupt {
            self.csrs[MIDELEG as usize]
        } else {
            self.csrs[MEDELEG as usize]
        };
        let delegated = (deleg >> cause_code) & 1 != 0;

        if delegated && self.privilege <= 1 {
            // Trap 到 S-mode
            self.write_csr(SEPC, self.pc);
            self.write_csr(SCAUSE, cause);
            self.write_csr(STVAL, tval);

            let mstatus = self.read_csr(MSTATUS);
            let sie_bit = (mstatus & MSTATUS_SIE) != 0;
            let new_mstatus = (mstatus & !MSTATUS_SIE & !MSTATUS_SPIE & !MSTATUS_SPP)
                | (if sie_bit { MSTATUS_SPIE } else { 0 })
                | (if self.privilege == 1 { MSTATUS_SPP } else { 0 });
            self.write_csr(MSTATUS, new_mstatus);

            self.privilege = 1; // 切换到 S-mode
            let stvec = self.read_csr(STVEC);
            self.pc = stvec & !0x3;
        } else {
            // Trap 到 M-mode
            self.write_csr(MEPC, self.pc);
            self.write_csr(MCAUSE, cause);
            self.write_csr(MTVAL, tval);

            let mstatus = self.read_csr(MSTATUS);
            let mie_bit = (mstatus & MSTATUS_MIE) != 0;
            let new_mstatus = (mstatus & !MSTATUS_MIE & !MSTATUS_MPIE & !MSTATUS_MPP_MASK)
                | (if mie_bit { MSTATUS_MPIE } else { 0 })
                | ((self.privilege as u32) << MSTATUS_MPP_SHIFT);
            self.write_csr(MSTATUS, new_mstatus);

            self.privilege = 3; // 切换到 M-mode
            let mtvec = self.read_csr(MTVEC);
            self.pc = mtvec & !0x3;
        }
    }

    /// 执行 MRET
    pub fn mret(&mut self) {
        let mstatus = self.read_csr(MSTATUS);
        // 恢复特权级 ← MPP
        let mpp = ((mstatus & MSTATUS_MPP_MASK) >> MSTATUS_MPP_SHIFT) as u8;
        self.privilege = mpp;
        // 恢复 MIE ← MPIE, MPIE ← 1, MPP ← U(0)
        let mpie = (mstatus & MSTATUS_MPIE) != 0;
        let mut new_mstatus = (mstatus & !MSTATUS_MIE & !MSTATUS_MPIE & !MSTATUS_MPP_MASK)
            | (if mpie { MSTATUS_MIE } else { 0 })
            | MSTATUS_MPIE; // MPIE 设为 1
        // xPP ≠ M → MPRV = 0
        if mpp != 3 {
            new_mstatus &= !MSTATUS_MPRV;
        }
        self.write_csr(MSTATUS, new_mstatus);

        self.pc = self.read_csr(MEPC);
    }

    /// 执行 SRET
    pub fn sret(&mut self) {
        let mstatus = self.read_csr(MSTATUS);
        // 恢复特权级 ← SPP (1 bit: 0=U, 1=S)
        let spp = if (mstatus & MSTATUS_SPP) != 0 { 1u8 } else { 0u8 };
        self.privilege = spp;
        // 恢复 SIE ← SPIE, SPIE ← 1, SPP ← U(0)
        let spie = (mstatus & MSTATUS_SPIE) != 0;
        let new_mstatus = (mstatus & !MSTATUS_SIE & !MSTATUS_SPIE & !MSTATUS_SPP)
            | (if spie { MSTATUS_SIE } else { 0 })
            | MSTATUS_SPIE; // SPIE 设为 1
        self.write_csr(MSTATUS, new_mstatus);

        self.pc = self.read_csr(SEPC);
    }

    /// 递增 64 位性能计数器（mcycle + minstret）
    fn increment_counters(&mut self) {
        if self.suppress_instret {
            self.suppress_instret = false;
            // mcycle still increments even when instret is suppressed
            let cy_lo = self.csrs[MCYCLE as usize];
            let (new_cy, carry) = cy_lo.overflowing_add(1);
            self.csrs[MCYCLE as usize] = new_cy;
            if carry {
                self.csrs[MCYCLEH as usize] = self.csrs[MCYCLEH as usize].wrapping_add(1);
            }
            return;
        }
        // minstret: 每条指令 +1
        let lo = self.csrs[MINSTRET as usize];
        let (new_lo, carry) = lo.overflowing_add(1);
        self.csrs[MINSTRET as usize] = new_lo;
        if carry {
            self.csrs[MINSTRETH as usize] = self.csrs[MINSTRETH as usize].wrapping_add(1);
        }
        // mcycle = minstret（单发射，每指令一周期）
        self.csrs[MCYCLE as usize] = new_lo;
        self.csrs[MCYCLEH as usize] = self.csrs[MINSTRETH as usize];
    }

    /// 返回 Load/Store 的有效特权级（处理 MPRV）
    fn effective_priv(&self) -> u8 {
        if self.privilege == 3 {
            let mstatus = self.csrs[MSTATUS as usize];
            if (mstatus & MSTATUS_MPRV) != 0 {
                return ((mstatus & MSTATUS_MPP_MASK) >> MSTATUS_MPP_SHIFT) as u8;
            }
        }
        self.privilege
    }

    /// Sv32 虚拟地址翻译。返回物理地址或 (cause, tval) 页错误
    pub fn translate(&self, bus: &Bus, va: u32, access: AccessType) -> Result<u32, (u32, u32)> {
        let priv_level = match access {
            AccessType::Execute => self.privilege,
            _ => self.effective_priv(),
        };
        // M-mode 不翻译
        if priv_level == 3 {
            return Ok(va);
        }

        let satp = self.csrs[SATP as usize];
        if (satp >> 31) == 0 {
            return Ok(va); // Bare 模式
        }

        let fault_cause = match access {
            AccessType::Execute => CAUSE_INST_PAGE_FAULT,
            AccessType::Read    => CAUSE_LOAD_PAGE_FAULT,
            AccessType::Write   => CAUSE_STORE_PAGE_FAULT,
        };

        // Sv32 两级页表遍历
        let root_ppn = satp & 0x003F_FFFF;
        let vpn = [(va >> 12) & 0x3FF, (va >> 22) & 0x3FF];
        let offset = va & 0xFFF;
        let mut a = root_ppn << 12;
        let mut level: i32 = 1;

        let pte = loop {
            let pte_addr = a.wrapping_add(vpn[level as usize] * 4);
            let pte = bus.ram.read32(pte_addr);

            // 无效 PTE 或保留编码 (W=1, R=0)
            if (pte & PTE_V) == 0 || ((pte & PTE_R) == 0 && (pte & PTE_W) != 0) {
                return Err((fault_cause, va));
            }

            // 叶子 PTE（R=1 或 X=1）
            if (pte & PTE_R) != 0 || (pte & PTE_X) != 0 {
                break pte;
            }

            // 非叶子：下一级
            level -= 1;
            if level < 0 {
                return Err((fault_cause, va));
            }
            a = ((pte >> 10) & 0x003F_FFFF) << 12;
        };

        let mstatus = self.csrs[MSTATUS as usize];

        // 权限检查
        match access {
            AccessType::Execute => {
                if (pte & PTE_X) == 0 { return Err((fault_cause, va)); }
            }
            AccessType::Read => {
                let mxr = (mstatus & MSTATUS_MXR) != 0;
                if (pte & PTE_R) == 0 && !(mxr && (pte & PTE_X) != 0) {
                    return Err((fault_cause, va));
                }
            }
            AccessType::Write => {
                if (pte & PTE_W) == 0 { return Err((fault_cause, va)); }
            }
        }

        // U/S 权限
        if (pte & PTE_U) != 0 {
            if priv_level == 1 && (mstatus & MSTATUS_SUM) == 0 {
                return Err((fault_cause, va));
            }
        } else if priv_level == 0 {
            return Err((fault_cause, va));
        }

        // 超级页对齐检查：level 1 时 PPN[0] 必须为 0
        if level == 1 && ((pte >> 10) & 0x3FF) != 0 {
            return Err((fault_cause, va));
        }

        // A/D 位检查（不在硬件中设置，触发页错误让软件处理）
        if (pte & PTE_A) == 0 {
            return Err((fault_cause, va));
        }
        if matches!(access, AccessType::Write) && (pte & PTE_D) == 0 {
            return Err((fault_cause, va));
        }

        // 构造物理地址
        let pa = if level == 1 {
            // 超级页：PA = PPN[1] : VPN[0] : offset
            let ppn1 = (pte >> 20) & 0xFFF;
            (ppn1 << 22) | (vpn[0] << 12) | offset
        } else {
            // 4KB 页：PA = PPN : offset
            let ppn = (pte >> 10) & 0x003F_FFFF;
            (ppn << 12) | offset
        };

        Ok(pa)
    }

    /// 取指 → 译码 → 执行
    pub fn step(&mut self, bus: &mut Bus) {
        if self.pc & 0x3 != 0 {
            self.trap(CAUSE_INST_MISALIGNED, self.pc);
            self.increment_counters();
            return;
        }
        let phys_pc = match self.translate(bus, self.pc, AccessType::Execute) {
            Ok(pa) => pa,
            Err((cause, tval)) => {
                self.trap(cause, tval);
                self.increment_counters();
                return;
            }
        };
        let raw = bus.read32(phys_pc);
        let inst = decode::decode(raw);
        execute::execute(self, bus, inst);
        self.increment_counters();
    }

    /// 运行指定周期数，返回 tohost 值（如果有）
    pub fn run(&mut self, bus: &mut Bus, max_cycles: u64) -> Option<u32> {
        for _ in 0..max_cycles {
            self.step(bus);
            if let Some(val) = bus.tohost_value {
                return Some(val);
            }
        }
        None
    }
}
