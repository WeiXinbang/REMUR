use crate::memory::Memory;

/// 系统总线：连接 CPU 和各外设
/// 当前仅包含 RAM，后续将添加 UART、PLIC 等 MMIO 设备
pub struct Bus {
    pub ram: Memory,
    pub tohost_addr: Option<u32>,
    pub tohost_value: Option<u32>,
}

impl Bus {
    pub fn new(ram: Memory) -> Self {
        Bus { ram, tohost_addr: None, tohost_value: None }
    }

    pub fn read8(&self, addr: u32) -> u8 {
        self.ram.read8(addr)
    }

    pub fn read16(&self, addr: u32) -> u16 {
        self.ram.read16(addr)
    }

    pub fn read32(&self, addr: u32) -> u32 {
        self.ram.read32(addr)
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        self.ram.write8(addr, val);
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        self.ram.write16(addr, val);
    }

    pub fn write32(&mut self, addr: u32, val: u32) {
        // tohost 检测（riscv-tests 用）
        if let Some(tohost) = self.tohost_addr {
            if addr == tohost && val != 0 {
                self.tohost_value = Some(val);
            }
        }
        self.ram.write32(addr, val);
    }
}
