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

// ── Device trait ──────────────────────────────────────────
/// 所有 MMIO 设备的统一接口
///
/// offset 为相对设备基地址的偏移量，由 Bus 负责映射。
/// 提供默认实现（读返回 0，写忽略），设备只需覆盖用到的方法。
/// MMIO 读取使用 &mut self，因为硬件读取常有副作用
/// （如 PLIC claim 清除 pending，UART RBR 消费接收缓冲）
pub trait Device {
    fn read8(&mut self, _offset: u32) -> u8 { 0 }
    fn read16(&mut self, _offset: u32) -> u16 { 0 }
    fn read32(&mut self, _offset: u32) -> u32 { 0 }
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
        }
    }

    pub fn read8(&mut self, addr: u32) -> u8 {
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.read8(addr - CLINT_BASE),
            PLIC_BASE..=PLIC_END => self.plic.read8(addr - PLIC_BASE),
            UART_BASE..=UART_END => self.uart.read8(addr - UART_BASE),
            RAM_BASE.. => self.ram.read8(addr - RAM_BASE),
            _ => 0,
        }
    }

    pub fn read16(&mut self, addr: u32) -> u16 {
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.read16(addr - CLINT_BASE),
            PLIC_BASE..=PLIC_END => self.plic.read16(addr - PLIC_BASE),
            UART_BASE..=UART_END => self.uart.read16(addr - UART_BASE),
            RAM_BASE.. => self.ram.read16(addr - RAM_BASE),
            _ => 0,
        }
    }

    pub fn read32(&mut self, addr: u32) -> u32 {
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.read32(addr - CLINT_BASE),
            PLIC_BASE..=PLIC_END => self.plic.read32(addr - PLIC_BASE),
            UART_BASE..=UART_END => self.uart.read32(addr - UART_BASE),
            RAM_BASE.. => self.ram.read32(addr - RAM_BASE),
            _ => 0,
        }
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.write8(addr - CLINT_BASE, val),
            PLIC_BASE..=PLIC_END => self.plic.write8(addr - PLIC_BASE, val),
            UART_BASE..=UART_END => self.uart.write8(addr - UART_BASE, val),
            RAM_BASE.. => self.ram.write8(addr - RAM_BASE, val),
            _ => {}
        }
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.write16(addr - CLINT_BASE, val),
            PLIC_BASE..=PLIC_END => self.plic.write16(addr - PLIC_BASE, val),
            UART_BASE..=UART_END => self.uart.write16(addr - UART_BASE, val),
            RAM_BASE.. => self.ram.write16(addr - RAM_BASE, val),
            _ => {}
        }
    }

    pub fn write32(&mut self, addr: u32, val: u32) {
        // tohost 检测（riscv-tests 用）
        if let Some(tohost) = self.tohost_addr {
            if addr == tohost && val != 0 {
                self.tohost_value = Some(val);
            }
        }
        match addr {
            CLINT_BASE..=CLINT_END => self.clint.write32(addr - CLINT_BASE, val),
            PLIC_BASE..=PLIC_END => self.plic.write32(addr - PLIC_BASE, val),
            UART_BASE..=UART_END => self.uart.write32(addr - UART_BASE, val),
            RAM_BASE.. => self.ram.write32(addr - RAM_BASE, val),
            _ => {}
        }
    }
}
