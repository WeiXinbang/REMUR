/// 物理内存，字节寻址，小端序
pub struct Memory {
    data: Vec<u8>,
    base: u32, // 内存基地址（例如 0x8000_0000）
}

impl Memory {
    pub fn new(size: usize, base: u32) -> Self {
        Memory {
            data: vec![0; size],
            base,
        }
    }

    /// 将地址转换为 data 数组的下标
    fn offset(&self, addr: u32) -> usize {
        let off = addr.wrapping_sub(self.base) as usize;
        assert!(
            off < self.data.len(),
            "Memory access out of bounds: addr=0x{:08x}, base=0x{:08x}, size=0x{:x}",
            addr, self.base, self.data.len()
        );
        off
    }

    pub fn read8(&self, addr: u32) -> u8 {
        self.data[self.offset(addr)]
    }

    pub fn read16(&self, addr: u32) -> u16 {
        let off = self.offset(addr);
        u16::from_le_bytes([self.data[off], self.data[off + 1]])
    }

    pub fn read32(&self, addr: u32) -> u32 {
        let off = self.offset(addr);
        u32::from_le_bytes([
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
        ])
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        let off = self.offset(addr);
        self.data[off] = val;
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        let off = self.offset(addr);
        let bytes = val.to_le_bytes();
        self.data[off] = bytes[0];
        self.data[off + 1] = bytes[1];
    }

    pub fn write32(&mut self, addr: u32, val: u32) {
        let off = self.offset(addr);
        let bytes = val.to_le_bytes();
        self.data[off] = bytes[0];
        self.data[off + 1] = bytes[1];
        self.data[off + 2] = bytes[2];
        self.data[off + 3] = bytes[3];
    }

    /// 加载二进制数据到指定地址
    pub fn load_binary(&mut self, addr: u32, data: &[u8]) {
        let off = self.offset(addr);
        self.data[off..off + data.len()].copy_from_slice(data);
    }
}
