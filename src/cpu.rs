//! Hart 核心实现：
//! - 通用寄存器 / CSR / 特权级状态
//! - trap / 中断 / SBI / Sv32 地址翻译
//! - `step()` 单步执行主循环与调试快照导出

#![allow(dead_code)]

use crate::bus::Bus;
#[cfg(feature = "cached-decode")]
use crate::cache::DecodeCache;
use crate::decode;
use crate::execute;
use crate::instruction::Instruction;

// CSR 地址常量
pub const MSTATUS: u16 = 0x300;
pub const MISA: u16 = 0x301;
pub const MEDELEG: u16 = 0x302;
pub const MIDELEG: u16 = 0x303;
pub const MIE: u16 = 0x304;
pub const MTVEC: u16 = 0x305;
pub const MCOUNTEREN: u16 = 0x306;
pub const MSCRATCH: u16 = 0x340;
pub const MEPC: u16 = 0x341;
pub const MCAUSE: u16 = 0x342;
pub const MTVAL: u16 = 0x343;
pub const MIP: u16 = 0x344;
pub const PMPCFG0: u16 = 0x3A0;
pub const PMPADDR0: u16 = 0x3B0;
pub const MHARTID: u16 = 0xF14;
pub const MCYCLE: u16 = 0xB00;
pub const MINSTRET: u16 = 0xB02;
pub const MCYCLEH: u16 = 0xB80;
pub const MINSTRETH: u16 = 0xB82;

// 只读影子计数器（U/S-mode 可访问，受 mcounteren 限制）
pub const CYCLE: u16 = 0xC00;
pub const TIME: u16 = 0xC01;
pub const INSTRET: u16 = 0xC02;
pub const CYCLEH: u16 = 0xC80;
pub const TIMEH: u16 = 0xC81;
pub const INSTRETH: u16 = 0xC82;

// S-mode CSR 地址
pub const SSTATUS: u16 = 0x100;
pub const SIE: u16 = 0x104;
pub const STVEC: u16 = 0x105;
pub const SCOUNTEREN: u16 = 0x106;
pub const SSCRATCH: u16 = 0x140;
pub const SEPC: u16 = 0x141;
pub const SCAUSE: u16 = 0x142;
pub const STVAL: u16 = 0x143;
pub const SIP: u16 = 0x144;
pub const SATP: u16 = 0x180;

// sstatus 可见位掩码（S-mode 只能看到这些 mstatus 位）
pub const SSTATUS_MASK: u32 =
    MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP | MSTATUS_FS_MASK | MSTATUS_SUM | MSTATUS_MXR;

// sie/sip 可见位掩码（S-mode 中断位：SSIE=1, STIE=5, SEIE=9）
pub const SIE_MASK: u32 = (1 << 1) | (1 << 5) | (1 << 9);

// mstatus 位域
pub const MSTATUS_SIE: u32 = 1 << 1;
pub const MSTATUS_MIE: u32 = 1 << 3;
pub const MSTATUS_SPIE: u32 = 1 << 5;
pub const MSTATUS_MPIE: u32 = 1 << 7;
pub const MSTATUS_SPP: u32 = 1 << 8;
pub const MSTATUS_MPP_MASK: u32 = 0x3 << 11;
pub const MSTATUS_MPP_SHIFT: u32 = 11;
pub const MSTATUS_FS_MASK: u32 = 0x3 << 13;
pub const MSTATUS_MPRV: u32 = 1 << 17;
pub const MSTATUS_SUM: u32 = 1 << 18;
pub const MSTATUS_MXR: u32 = 1 << 19;
pub const MSTATUS_TVM: u32 = 1 << 20;
pub const MSTATUS_TW: u32 = 1 << 21;
pub const MSTATUS_TSR: u32 = 1 << 22;

// MIP/MIE 位域（中断挂起/使能）
pub const MIP_SSIP: u32 = 1 << 1; // S-mode 软件中断
pub const MIP_MSIP: u32 = 1 << 3; // M-mode 软件中断
pub const MIP_STIP: u32 = 1 << 5; // S-mode 定时器中断
pub const MIP_MTIP: u32 = 1 << 7; // M-mode 定时器中断
pub const MIP_SEIP: u32 = 1 << 9; // S-mode 外部中断
pub const MIP_MEIP: u32 = 1 << 11; // M-mode 外部中断

// 硬件控制的 MIP 位（CSR 写入时保护）
const MIP_HW_MASK: u32 = MIP_MTIP | MIP_MSIP | MIP_SEIP | MIP_MEIP;

// 异常原因
pub const CAUSE_INST_MISALIGNED: u32 = 0;
pub const CAUSE_ILLEGAL_INST: u32 = 2;
pub const CAUSE_BREAKPOINT: u32 = 3;
pub const CAUSE_LOAD_MISALIGNED: u32 = 4;
pub const CAUSE_STORE_MISALIGNED: u32 = 6;
pub const CAUSE_ECALL_U: u32 = 8;
pub const CAUSE_ECALL_S: u32 = 9;
pub const CAUSE_ECALL_M: u32 = 11;
pub const CAUSE_INST_PAGE_FAULT: u32 = 12;
pub const CAUSE_LOAD_PAGE_FAULT: u32 = 13;
pub const CAUSE_STORE_PAGE_FAULT: u32 = 15;

// Sv32 页表项位域
const PTE_V: u32 = 1 << 0;
const PTE_R: u32 = 1 << 1;
const PTE_W: u32 = 1 << 2;
const PTE_X: u32 = 1 << 3;
const PTE_U: u32 = 1 << 4;
const PTE_A: u32 = 1 << 6;
const PTE_D: u32 = 1 << 7;

// ── 内嵌 SBI 常量 ───────────────────────────────────────────
const SBI_SUCCESS: i32 = 0;
const SBI_ERR_NOT_SUPPORTED: i32 = -2;
const SBI_CONSOLE_MMIO_ADDR: u32 = 0x1000_0000;

const SBI_LEGACY_SET_TIMER_EID: u32 = 0x00;
const SBI_LEGACY_CONSOLE_PUTCHAR_EID: u32 = 0x01;
const SBI_LEGACY_CONSOLE_GETCHAR_EID: u32 = 0x02;
const SBI_LEGACY_SHUTDOWN_EID: u32 = 0x08;
const SBI_BASE_EID: u32 = 0x10;
const SBI_DBCN_EID: u32 = 0x4442_434E;
const SBI_TIME_EID: u32 = 0x5449_4D45;
const SBI_SRST_EID: u32 = 0x5352_5354;

const SBI_BASE_GET_SPEC_VERSION_FID: u32 = 0;
const SBI_BASE_GET_IMPL_ID_FID: u32 = 1;
const SBI_BASE_GET_IMPL_VERSION_FID: u32 = 2;
const SBI_BASE_PROBE_EXTENSION_FID: u32 = 3;
const SBI_BASE_GET_MVENDORID_FID: u32 = 4;
const SBI_BASE_GET_MARCHID_FID: u32 = 5;
const SBI_BASE_GET_MIMPID_FID: u32 = 6;

const SBI_DBCN_CONSOLE_WRITE_FID: u32 = 0;
const SBI_DBCN_CONSOLE_WRITE_BYTE_FID: u32 = 2;
const SBI_TIME_SET_TIMER_FID: u32 = 0;
const SBI_SRST_SYSTEM_RESET_FID: u32 = 0;

#[derive(Clone, Copy, Debug)]
struct SbiCall {
    eid: u32,
    fid: u32,
    a0: u32,
    a1: u32,
    a2: u32,
}

impl SbiCall {
    fn from_hart(hart: &Hart) -> Self {
        Self {
            eid: hart.read_reg(17),
            fid: hart.read_reg(16),
            a0: hart.read_reg(10),
            a1: hart.read_reg(11),
            a2: hart.read_reg(12),
        }
    }

    fn a0_a1_u64(self) -> u64 {
        ((self.a1 as u64) << 32) | (self.a0 as u64)
    }
}

/// 内存访问类型（用于地址翻译）
pub enum AccessType {
    Execute,
    Read,
    Write,
}

#[derive(Debug, Clone, Copy)]
/// Trap 发生时的关键状态快照（用于 itrace/difftest 输出）。
pub struct TrapSnapshot {
    /// mcause/scause 原始值（含中断标志位）。
    pub cause: u32,
    /// mtval/stval 值。
    pub tval: u32,
    /// trap 前特权级（0=U,1=S,3=M）。
    pub from_privilege: u8,
    /// trap 后特权级。
    pub to_privilege: u8,
    /// 是否走了 MIDELEG/MEDELEG 委托到 S 态。
    pub delegated_to_s: bool,
    /// trap 后 PC（mtvec/stvec 生效后的入口）。
    pub trap_vector: u32,
}

#[derive(Debug, Clone, Copy)]
/// 单步执行摘要：用于 CLI trace 与最小 difftest 对拍。
pub struct StepSnapshot {
    /// 本步开始时 PC。
    pub pc: u32,
    /// 本步开始时特权级。
    pub privilege_before: u8,
    /// 取指后的物理地址（中断/取指异常时为空）。
    pub phys_pc: Option<u32>,
    /// 取到的原始指令字。
    pub raw_inst: Option<u32>,
    /// 已译码指令（用于测试和调试）。
    pub decoded_inst: Option<Instruction>,
    /// 本步结束后 PC。
    pub next_pc: u32,
    /// 本步结束后特权级。
    pub privilege_after: u8,
    /// 若本步入口先处理了中断，这里记录中断 cause。
    pub interrupt_cause: Option<u32>,
    /// 若本步触发 trap，这里附带 trap 快照。
    pub trap: Option<TrapSnapshot>,
}

/// RISC-V Hart（硬件线程）
pub struct Hart {
    pub regs: [u32; 32],
    pub pc: u32,
    pub csrs: [u32; 4096],
    pub privilege: u8,               // 当前特权级: 0=U, 1=S, 3=M
    pub reservation: Option<u32>,    // LR/SC 保留地址
    sbi_enabled: bool,               // Linux 启动路径：在 S-mode 直接拦截 SBI ecall
    shutdown_requested: bool,        // SBI shutdown/system_reset 请求
    sbi_timer_deadline: Option<u64>, // SBI set_timer 目标时间（用于生成 STIP）
    suppress_instret: bool,          // 写 minstret/minstreth 后抑制本次递增
    last_trap: Option<TrapSnapshot>,
    #[cfg(feature = "cached-decode")]
    decode_cache: DecodeCache,
}

impl Hart {
    pub fn new() -> Self {
        let mut hart = Hart {
            regs: [0; 32],
            pc: 0,
            csrs: [0; 4096],
            privilege: 3, // 复位为 M-mode
            reservation: None,
            sbi_enabled: false,
            shutdown_requested: false,
            sbi_timer_deadline: None,
            suppress_instret: false,
            last_trap: None,
            #[cfg(feature = "cached-decode")]
            decode_cache: DecodeCache::new(),
        };
        // misa: RV32IMA + S + U
        hart.csrs[MISA as usize] = (1 << 30)  // MXL=1 (32-bit)
            | (1 << 8)   // I
            | (1 << 12)  // M
            | (1 << 18)  // S
            | (1 << 20)  // U
            | (1 << 0); // A
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

    /// 清空译码缓存（SFENCE.VMA 时由 execute 调用）
    #[cfg(feature = "cached-decode")]
    pub fn flush_decode_cache(&mut self) {
        self.decode_cache.flush();
    }

    /// 启用/禁用内嵌 SBI（M6 Linux 启动路径）
    pub fn enable_sbi(&mut self, enabled: bool) {
        self.sbi_enabled = enabled;
    }

    /// 是否收到 SBI shutdown/system_reset 请求
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    fn can_handle_sbi_ecall(&self) -> bool {
        self.sbi_enabled && self.privilege == 1
    }

    fn program_sbi_timer(&mut self, bus: &mut Bus, deadline: u64) {
        self.sbi_timer_deadline = Some(deadline);
        bus.clint.mtimecmp = deadline;
        // set_timer 表示“重新编程下一次中断”，先清掉 STIP，后续由 update_mip 重新置位
        self.csrs[MIP as usize] &= !MIP_STIP;
    }

    fn sbi_set_result(&mut self, error: i32, value: u32) {
        self.write_reg(10, error as u32); // a0 = error
        self.write_reg(11, value); // a1 = value
    }

    fn sbi_probe_extension(ext: u32) -> u32 {
        match ext {
            SBI_BASE_EID | SBI_DBCN_EID | SBI_TIME_EID | SBI_SRST_EID => 1,
            _ => 0,
        }
    }

    fn handle_legacy_sbi_call(&mut self, bus: &mut Bus, call: SbiCall) {
        match call.eid {
            SBI_LEGACY_SET_TIMER_EID => {
                self.program_sbi_timer(bus, call.a0_a1_u64());
                self.write_reg(10, 0);
            }
            SBI_LEGACY_CONSOLE_PUTCHAR_EID => {
                bus.write8(SBI_CONSOLE_MMIO_ADDR, call.a0 as u8);
                self.write_reg(10, 0);
            }
            SBI_LEGACY_CONSOLE_GETCHAR_EID => {
                self.write_reg(10, u32::MAX); // -1: no input
            }
            SBI_LEGACY_SHUTDOWN_EID => {
                self.shutdown_requested = true;
                self.write_reg(10, 0);
            }
            _ => unreachable!("legacy SBI handler only accepts legacy EIDs"),
        }
    }

    fn handle_sbi_base_extension(&mut self, call: SbiCall) {
        match call.fid {
            SBI_BASE_GET_SPEC_VERSION_FID => self.sbi_set_result(SBI_SUCCESS, 0x0000_0002),
            SBI_BASE_GET_IMPL_ID_FID => self.sbi_set_result(SBI_SUCCESS, 0x5245_4D55),
            SBI_BASE_GET_IMPL_VERSION_FID => self.sbi_set_result(SBI_SUCCESS, 0x0000_0001),
            SBI_BASE_PROBE_EXTENSION_FID => {
                self.sbi_set_result(SBI_SUCCESS, Self::sbi_probe_extension(call.a0))
            }
            SBI_BASE_GET_MVENDORID_FID => self.sbi_set_result(SBI_SUCCESS, self.read_csr(0xF11)),
            SBI_BASE_GET_MARCHID_FID => self.sbi_set_result(SBI_SUCCESS, self.read_csr(0xF12)),
            SBI_BASE_GET_MIMPID_FID => self.sbi_set_result(SBI_SUCCESS, self.read_csr(0xF13)),
            _ => self.sbi_set_result(SBI_ERR_NOT_SUPPORTED, 0),
        }
    }

    fn handle_sbi_dbcn_extension(&mut self, bus: &mut Bus, call: SbiCall) {
        match call.fid {
            SBI_DBCN_CONSOLE_WRITE_FID => {
                if call.a2 != 0 {
                    self.sbi_set_result(SBI_ERR_NOT_SUPPORTED, 0);
                } else {
                    for i in 0..call.a0 {
                        let ch = bus.read8(call.a1.wrapping_add(i));
                        bus.write8(SBI_CONSOLE_MMIO_ADDR, ch);
                    }
                    self.sbi_set_result(SBI_SUCCESS, call.a0);
                }
            }
            SBI_DBCN_CONSOLE_WRITE_BYTE_FID => {
                bus.write8(SBI_CONSOLE_MMIO_ADDR, call.a0 as u8);
                self.sbi_set_result(SBI_SUCCESS, 0);
            }
            _ => self.sbi_set_result(SBI_ERR_NOT_SUPPORTED, 0),
        }
    }

    fn handle_sbi_time_extension(&mut self, bus: &mut Bus, call: SbiCall) {
        if call.fid == SBI_TIME_SET_TIMER_FID {
            self.program_sbi_timer(bus, call.a0_a1_u64());
            self.sbi_set_result(SBI_SUCCESS, 0);
        } else {
            self.sbi_set_result(SBI_ERR_NOT_SUPPORTED, 0);
        }
    }

    fn handle_sbi_reset_extension(&mut self, call: SbiCall) {
        if call.fid == SBI_SRST_SYSTEM_RESET_FID {
            self.shutdown_requested = true;
            self.sbi_set_result(SBI_SUCCESS, 0);
        } else {
            self.sbi_set_result(SBI_ERR_NOT_SUPPORTED, 0);
        }
    }

    /// 在 S-mode 拦截并处理 SBI ecall。
    /// 返回 true 表示该 ecall 已由内嵌 SBI 消化，不需要再走 trap 机制。
    pub fn handle_sbi_ecall(&mut self, bus: &mut Bus) -> bool {
        if !self.can_handle_sbi_ecall() {
            return false;
        }

        let call = SbiCall::from_hart(self);

        match call.eid {
            SBI_LEGACY_SET_TIMER_EID
            | SBI_LEGACY_CONSOLE_PUTCHAR_EID
            | SBI_LEGACY_CONSOLE_GETCHAR_EID
            | SBI_LEGACY_SHUTDOWN_EID => self.handle_legacy_sbi_call(bus, call),
            SBI_BASE_EID => self.handle_sbi_base_extension(call),
            SBI_DBCN_EID => self.handle_sbi_dbcn_extension(bus, call),
            SBI_TIME_EID => self.handle_sbi_time_extension(bus, call),
            SBI_SRST_EID => self.handle_sbi_reset_extension(call),
            _ => self.sbi_set_result(SBI_ERR_NOT_SUPPORTED, 0),
        }

        true
    }

    /// 读 CSR（处理 S-mode 别名）
    pub fn read_csr(&self, addr: u16) -> u32 {
        match addr {
            SSTATUS => self.csrs[MSTATUS as usize] & SSTATUS_MASK,
            SIE => self.csrs[MIE as usize] & SIE_MASK,
            SIP => self.csrs[MIP as usize] & SIE_MASK,
            // 只读影子计数器 → 读对应的 M-mode 计数器
            CYCLE => self.csrs[MCYCLE as usize],
            TIME => self.csrs[TIME as usize],
            INSTRET => self.csrs[MINSTRET as usize],
            CYCLEH => self.csrs[MCYCLEH as usize],
            TIMEH => self.csrs[TIMEH as usize],
            INSTRETH => self.csrs[MINSTRETH as usize],
            // Debug trigger：tselect 固定为 0（仅 trigger 0），tdata1 固定为 0（不支持任何触发类型）
            0x7A0 => 0, // tselect
            0x7A1 => 0, // tdata1
            _ => self.csrs[addr as usize],
        }
    }

    /// 写 CSR（处理只读和 S-mode 别名）
    pub fn write_csr(&mut self, addr: u16, val: u32) {
        match addr {
            MHARTID | MISA => {} // 只读
            MTVEC | STVEC => {
                self.csrs[addr as usize] = val & !0x3;
            } // 只支持 Direct 模式
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
                // 保护硬件位，只允许写入 S-mode 位中非硬件控制的部分
                let writable = SIE_MASK & !MIP_HW_MASK;
                self.csrs[MIP as usize] = (mip & !writable) | (val & writable);
            }
            MIP => {
                // 保护硬件控制的位（MTIP、MSIP、MEIP）
                let hw_bits = self.csrs[MIP as usize] & MIP_HW_MASK;
                self.csrs[MIP as usize] = (val & !MIP_HW_MASK) | hw_bits;
            }
            MINSTRET | MINSTRETH => {
                self.csrs[addr as usize] = val;
                self.suppress_instret = true;
            }
            SATP => {
                self.csrs[addr as usize] = val;
                #[cfg(feature = "cached-decode")]
                self.decode_cache.flush();
            }
            _ => {
                self.csrs[addr as usize] = val;
            }
        }
    }

    /// 触发 trap：根据 medeleg 决定 trap 到 M-mode 还是 S-mode
    pub fn trap(&mut self, cause: u32, tval: u32) {
        let from_privilege = self.privilege;
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

        self.last_trap = Some(TrapSnapshot {
            cause,
            tval,
            from_privilege,
            to_privilege: self.privilege,
            delegated_to_s: delegated && from_privilege <= 1,
            trap_vector: self.pc,
        });
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
        let spp = if (mstatus & MSTATUS_SPP) != 0 {
            1u8
        } else {
            0u8
        };
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
    pub fn translate(&self, bus: &mut Bus, va: u32, access: AccessType) -> Result<u32, (u32, u32)> {
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
            AccessType::Read => CAUSE_LOAD_PAGE_FAULT,
            AccessType::Write => CAUSE_STORE_PAGE_FAULT,
        };

        // Sv32 两级页表遍历
        let root_ppn = satp & 0x003F_FFFF;
        let vpn = [(va >> 12) & 0x3FF, (va >> 22) & 0x3FF];
        let offset = va & 0xFFF;
        let mut a = root_ppn << 12;
        let mut level: i32 = 1;

        let pte = loop {
            let pte_addr = a.wrapping_add(vpn[level as usize] * 4);
            let pte = bus.read32(pte_addr);

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
                if (pte & PTE_X) == 0 {
                    return Err((fault_cause, va));
                }
            }
            AccessType::Read => {
                let mxr = (mstatus & MSTATUS_MXR) != 0;
                if (pte & PTE_R) == 0 && !(mxr && (pte & PTE_X) != 0) {
                    return Err((fault_cause, va));
                }
            }
            AccessType::Write => {
                if (pte & PTE_W) == 0 {
                    return Err((fault_cause, va));
                }
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

    /// 从 CLINT/PLIC 同步硬件中断位到 MIP
    fn update_mip(&mut self, bus: &mut Bus) {
        let mut mip = self.csrs[MIP as usize];
        // MTIP: 由 CLINT 定时器控制
        if bus.clint.timer_interrupt_pending() {
            mip |= MIP_MTIP;
        } else {
            mip &= !MIP_MTIP;
        }
        // MSIP: 由 CLINT 软件中断控制
        if bus.clint.software_interrupt_pending() {
            mip |= MIP_MSIP;
        } else {
            mip &= !MIP_MSIP;
        }
        // MEIP: context0 (M-mode external interrupt)
        if bus.plic.has_pending_m_interrupt() {
            mip |= MIP_MEIP;
        } else {
            mip &= !MIP_MEIP;
        }
        // SEIP: context1 (S-mode external interrupt)
        if bus.plic.has_pending_s_interrupt() {
            mip |= MIP_SEIP;
        } else {
            mip &= !MIP_SEIP;
        }
        // STIP: 由内嵌 SBI set_timer 逻辑驱动
        if let Some(deadline) = self.sbi_timer_deadline {
            if bus.clint.mtime >= deadline {
                mip |= MIP_STIP;
            } else {
                mip &= !MIP_STIP;
            }
        }
        self.csrs[MIP as usize] = mip;
    }

    /// 检查待处理中断，返回最高优先级中断的 cause（含 MSB 中断标志）
    fn check_pending_interrupts(&self) -> Option<u32> {
        let mip = self.csrs[MIP as usize];
        let mie_reg = self.csrs[MIE as usize];
        let pending = mip & mie_reg;
        if pending == 0 {
            return None;
        }

        let mstatus = self.csrs[MSTATUS as usize];
        let mideleg = self.csrs[MIDELEG as usize];

        // 非委托中断（trap 到 M-mode）：priv < M，或 priv == M 且 MIE=1
        let m_ie = match self.privilege {
            3 => (mstatus & MSTATUS_MIE) != 0,
            _ => true,
        };
        // 委托中断（trap 到 S-mode）：priv < S，或 priv == S 且 SIE=1；M-mode 不响应
        let s_ie = match self.privilege {
            3 => false,
            1 => (mstatus & MSTATUS_SIE) != 0,
            _ => true,
        };

        let can_take = (pending & !mideleg & if m_ie { u32::MAX } else { 0 })
            | (pending & mideleg & if s_ie { u32::MAX } else { 0 });
        if can_take == 0 {
            return None;
        }

        // 优先级: MEI(11) > MSI(3) > MTI(7) > SEI(9) > SSI(1) > STI(5)
        for bit in [11, 3, 7, 9, 1, 5] {
            if (can_take >> bit) & 1 != 0 {
                return Some(0x8000_0000 | bit);
            }
        }
        None
    }

    /// 取指 → 译码 → 执行，并返回本周期快照（M7 itrace/difftest 使用）
    pub fn step_snapshot(&mut self, bus: &mut Bus) -> StepSnapshot {
        let pc = self.pc;
        let privilege_before = self.privilege;
        self.last_trap = None;

        // 推进 CLINT 时钟 & 同步硬件中断位
        bus.clint.tick();
        self.csrs[TIME as usize] = bus.clint.mtime as u32;
        self.csrs[TIMEH as usize] = (bus.clint.mtime >> 32) as u32;
        bus.poll_host_io();
        self.update_mip(bus);

        // 检查待处理中断
        if let Some(cause) = self.check_pending_interrupts() {
            self.trap(cause, 0);
            return StepSnapshot {
                pc,
                privilege_before,
                phys_pc: None,
                raw_inst: None,
                decoded_inst: None,
                next_pc: self.pc,
                privilege_after: self.privilege,
                interrupt_cause: Some(cause),
                trap: self.last_trap,
            };
        }

        if self.pc & 0x3 != 0 {
            self.trap(CAUSE_INST_MISALIGNED, self.pc);
            self.increment_counters();
            return StepSnapshot {
                pc,
                privilege_before,
                phys_pc: None,
                raw_inst: None,
                decoded_inst: None,
                next_pc: self.pc,
                privilege_after: self.privilege,
                interrupt_cause: None,
                trap: self.last_trap,
            };
        }
        let phys_pc = match self.translate(bus, self.pc, AccessType::Execute) {
            Ok(pa) => pa,
            Err((cause, tval)) => {
                self.trap(cause, tval);
                self.increment_counters();
                return StepSnapshot {
                    pc,
                    privilege_before,
                    phys_pc: None,
                    raw_inst: None,
                    decoded_inst: None,
                    next_pc: self.pc,
                    privilege_after: self.privilege,
                    interrupt_cause: None,
                    trap: self.last_trap,
                };
            }
        };

        let raw = bus.read32(phys_pc);

        #[cfg(feature = "cached-decode")]
        let inst = if let Some(cached) = self.decode_cache.lookup(phys_pc) {
            cached
        } else {
            let decoded = decode::decode(raw);
            self.decode_cache.insert(phys_pc, decoded);
            decoded
        };

        #[cfg(not(feature = "cached-decode"))]
        let inst = decode::decode(raw);

        execute::execute(self, bus, inst);
        self.increment_counters();

        StepSnapshot {
            pc,
            privilege_before,
            phys_pc: Some(phys_pc),
            raw_inst: Some(raw),
            decoded_inst: Some(inst),
            next_pc: self.pc,
            privilege_after: self.privilege,
            interrupt_cause: None,
            trap: self.last_trap,
        }
    }

    /// 取指 → 译码 → 执行
    pub fn step(&mut self, bus: &mut Bus) {
        let _ = self.step_snapshot(bus);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;

    fn new_sbi_context() -> (Hart, Bus) {
        let mut hart = Hart::new();
        hart.privilege = 1;
        hart.enable_sbi(true);
        let bus = Bus::new(Memory::new(1024 * 1024));
        (hart, bus)
    }

    #[test]
    fn sbi_ecall_requires_enabled_s_mode() {
        let mut hart = Hart::new();
        let mut bus = Bus::new(Memory::new(1024 * 1024));
        hart.write_reg(17, SBI_LEGACY_SHUTDOWN_EID);

        assert!(!hart.handle_sbi_ecall(&mut bus));

        hart.enable_sbi(true);
        assert!(!hart.handle_sbi_ecall(&mut bus));

        hart.privilege = 1;
        assert!(hart.handle_sbi_ecall(&mut bus));
        assert!(hart.shutdown_requested());
    }

    #[test]
    fn sbi_reset_extension_sets_shutdown_and_success_result() {
        let (mut hart, mut bus) = new_sbi_context();
        hart.write_reg(16, SBI_SRST_SYSTEM_RESET_FID);
        hart.write_reg(17, SBI_SRST_EID);

        assert!(hart.handle_sbi_ecall(&mut bus));
        assert!(hart.shutdown_requested());
        assert_eq!(hart.read_reg(10), SBI_SUCCESS as u32);
        assert_eq!(hart.read_reg(11), 0);
    }

    #[test]
    fn sbi_dbcn_console_write_rejects_nonzero_high_address() {
        let (mut hart, mut bus) = new_sbi_context();
        bus.uart.enable_capture();
        hart.write_reg(10, 4);
        hart.write_reg(11, 0x8000_1000);
        hart.write_reg(12, 1);
        hart.write_reg(16, SBI_DBCN_CONSOLE_WRITE_FID);
        hart.write_reg(17, SBI_DBCN_EID);

        assert!(hart.handle_sbi_ecall(&mut bus));
        assert_eq!(hart.read_reg(10), SBI_ERR_NOT_SUPPORTED as u32);
        assert_eq!(hart.read_reg(11), 0);
        assert_eq!(bus.uart.pop_output(), None);
    }

    #[test]
    fn update_mip_sets_stip_after_sbi_timer_deadline() {
        let mut hart = Hart::new();
        let mut bus = Bus::new(Memory::new(1024 * 1024));
        hart.sbi_timer_deadline = Some(5);

        bus.clint.mtime = 4;
        hart.update_mip(&mut bus);
        assert_eq!(hart.read_csr(MIP) & MIP_STIP, 0);

        bus.clint.mtime = 5;
        hart.update_mip(&mut bus);
        assert_eq!(hart.read_csr(MIP) & MIP_STIP, MIP_STIP);
    }
}
