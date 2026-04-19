use crate::clint::Clint;
use crate::memory::Memory;
use crate::plic::Plic;
use crate::uart::Uart;

// ── 地址映射常量 ──────────────────────────────────────────
pub const CLINT_BASE: u32 = 0x0200_0000;
const CLINT_END: u32 = 0x0200_FFFF;
pub const PLIC_BASE: u32 = 0x0C00_0000;
const PLIC_END: u32 = 0x0FFF_FFFF;
const UART_BASE: u32 = 0x1000_0000;
const UART_END: u32 = 0x1000_0FFF;
pub const RAM_BASE: u32 = 0x8000_0000;
const RV32EMU_SOC_BASE: u32 = 0xF000_0000;
const RV32EMU_PLIC_BASE: u32 = RV32EMU_SOC_BASE;
const RV32EMU_PLIC_END: u32 = RV32EMU_SOC_BASE + 0x03FF_FFFF;
const RV32EMU_UART_BASE: u32 = RV32EMU_SOC_BASE + 0x0400_0000;
const RV32EMU_UART_END: u32 = RV32EMU_SOC_BASE + 0x040F_FFFF;

// ── Device trait ──────────────────────────────────────────
/// 所有 MMIO 设备的统一接口
///
/// offset 为相对设备基地址的偏移量，由 Bus 负责映射。
/// 提供默认实现（读返回 0，写忽略），设备只需覆盖用到的方法。
/// MMIO 读取使用 &mut self，因为硬件读取常有副作用
/// （如 PLIC claim 清除 pending，UART RBR 消费接收缓冲）
pub trait Device {
    fn read8(&mut self, _offset: u32) -> u8 {
        0
    }
    fn read16(&mut self, _offset: u32) -> u16 {
        0
    }
    fn read32(&mut self, _offset: u32) -> u32 {
        0
    }
    fn write8(&mut self, _offset: u32, _val: u8) {}
    fn write16(&mut self, _offset: u32, _val: u16) {}
    fn write32(&mut self, _offset: u32, _val: u32) {}
}

// ── 系统总线 ──────────────────────────────────────────────
/// 系统总线：地址路由 + 设备互连
///
/// 持有具体类型字段（零开销静态分派），用 match 路由地址。
pub struct Bus {
    pub ram: Memory,
    pub uart: Uart,
    pub clint: Clint,
    pub plic: Plic,
    pub tohost_addr: Option<u32>,
    pub tohost_value: Option<u32>,
    low_ram_alias: bool,
    rv32emu_mmio_alias: bool,
    uart_irq: u32,
}

impl Bus {
    pub fn new(ram: Memory) -> Self {
        Bus {
            ram,
            uart: Uart::new(),
            clint: Clint::new(),
            plic: Plic::new(),
            tohost_addr: None,
            tohost_value: None,
            low_ram_alias: false,
            rv32emu_mmio_alias: false,
            uart_irq: 10,
        }
    }

    pub fn enable_rv32emu_linux_compat(&mut self) {
        self.low_ram_alias = true;
        self.rv32emu_mmio_alias = true;
        self.uart_irq = 1;
    }

    pub fn is_low_ram_alias_enabled(&self) -> bool {
        self.low_ram_alias
    }

    fn ram_offset(&self, addr: u32) -> Option<u32> {
        if addr >= RAM_BASE {
            return Some(addr - RAM_BASE);
        }
        if self.low_ram_alias && addr < self.ram.size() {
            return Some(addr);
        }
        None
    }

    pub fn read8(&mut self, addr: u32) -> u8 {
        if self.rv32emu_mmio_alias {
            match addr {
                RV32EMU_PLIC_BASE..=RV32EMU_PLIC_END => {
                    return self.plic.read8(addr - RV32EMU_PLIC_BASE);
                }
                RV32EMU_UART_BASE..=RV32EMU_UART_END => {
                    return self.uart.read8(addr - RV32EMU_UART_BASE);
                }
                _ => {}
            }
        }
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.read8(addr - CLINT_BASE),
            PLIC_BASE..=PLIC_END => self.plic.read8(addr - PLIC_BASE),
            UART_BASE..=UART_END => self.uart.read8(addr - UART_BASE),
            _ => self.ram_offset(addr).map_or(0, |off| self.ram.read8(off)),
        }
    }

    pub fn read16(&mut self, addr: u32) -> u16 {
        if self.rv32emu_mmio_alias {
            match addr {
                RV32EMU_PLIC_BASE..=RV32EMU_PLIC_END => {
                    return self.plic.read16(addr - RV32EMU_PLIC_BASE);
                }
                RV32EMU_UART_BASE..=RV32EMU_UART_END => {
                    return self.uart.read16(addr - RV32EMU_UART_BASE);
                }
                _ => {}
            }
        }
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.read16(addr - CLINT_BASE),
            PLIC_BASE..=PLIC_END => self.plic.read16(addr - PLIC_BASE),
            UART_BASE..=UART_END => self.uart.read16(addr - UART_BASE),
            _ => self.ram_offset(addr).map_or(0, |off| self.ram.read16(off)),
        }
    }

    pub fn read32(&mut self, addr: u32) -> u32 {
        if self.rv32emu_mmio_alias {
            match addr {
                RV32EMU_PLIC_BASE..=RV32EMU_PLIC_END => {
                    return self.plic.read32(addr - RV32EMU_PLIC_BASE);
                }
                RV32EMU_UART_BASE..=RV32EMU_UART_END => {
                    return self.uart.read32(addr - RV32EMU_UART_BASE);
                }
                _ => {}
            }
        }
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.read32(addr - CLINT_BASE),
            PLIC_BASE..=PLIC_END => self.plic.read32(addr - PLIC_BASE),
            UART_BASE..=UART_END => self.uart.read32(addr - UART_BASE),
            _ => self.ram_offset(addr).map_or(0, |off| self.ram.read32(off)),
        }
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        if self.rv32emu_mmio_alias {
            match addr {
                RV32EMU_PLIC_BASE..=RV32EMU_PLIC_END => {
                    self.plic.write8(addr - RV32EMU_PLIC_BASE, val);
                    return;
                }
                RV32EMU_UART_BASE..=RV32EMU_UART_END => {
                    self.uart.write8(addr - RV32EMU_UART_BASE, val);
                    return;
                }
                _ => {}
            }
        }
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.write8(addr - CLINT_BASE, val),
            PLIC_BASE..=PLIC_END => self.plic.write8(addr - PLIC_BASE, val),
            UART_BASE..=UART_END => self.uart.write8(addr - UART_BASE, val),
            _ => {
                if let Some(off) = self.ram_offset(addr) {
                    self.ram.write8(off, val);
                }
            }
        }
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        if self.rv32emu_mmio_alias {
            match addr {
                RV32EMU_PLIC_BASE..=RV32EMU_PLIC_END => {
                    self.plic.write16(addr - RV32EMU_PLIC_BASE, val);
                    return;
                }
                RV32EMU_UART_BASE..=RV32EMU_UART_END => {
                    self.uart.write16(addr - RV32EMU_UART_BASE, val);
                    return;
                }
                _ => {}
            }
        }
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.write16(addr - CLINT_BASE, val),
            PLIC_BASE..=PLIC_END => self.plic.write16(addr - PLIC_BASE, val),
            UART_BASE..=UART_END => self.uart.write16(addr - UART_BASE, val),
            _ => {
                if let Some(off) = self.ram_offset(addr) {
                    self.ram.write16(off, val);
                }
            }
        }
    }

    pub fn write32(&mut self, addr: u32, val: u32) {
        // tohost 检测（riscv-tests 用）
        if let Some(tohost) = self.tohost_addr {
            if addr == tohost && val != 0 {
                self.tohost_value = Some(val);
            }
        }

        if self.rv32emu_mmio_alias {
            match addr {
                RV32EMU_PLIC_BASE..=RV32EMU_PLIC_END => {
                    self.plic.write32(addr - RV32EMU_PLIC_BASE, val);
                    return;
                }
                RV32EMU_UART_BASE..=RV32EMU_UART_END => {
                    self.uart.write32(addr - RV32EMU_UART_BASE, val);
                    return;
                }
                _ => {}
            }
        }

        match addr {
            CLINT_BASE..=CLINT_END => self.clint.write32(addr - CLINT_BASE, val),
            PLIC_BASE..=PLIC_END => self.plic.write32(addr - PLIC_BASE, val),
            UART_BASE..=UART_END => self.uart.write32(addr - UART_BASE, val),
            _ => {
                if let Some(off) = self.ram_offset(addr) {
                    self.ram.write32(off, val);
                }
            }
        }
    }

    /// 轮询主机输入并在需要时向 PLIC 注入 UART IRQ。
    pub fn poll_host_io(&mut self) {
        if self.uart.poll_host_input() {
            self.plic.set_pending(self.uart_irq);
        }
    }
}
