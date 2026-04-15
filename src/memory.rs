/// 物理内存，字节寻址，小端序
///
/// 所有地址使用相对偏移（0-based），由 Bus 负责地址映射。
pub struct Memory {
    data: Vec<u8>,
}

impl Memory {
    pub fn new(size: usize) -> Self {
        Memory {
            data: vec![0; size],
        }
    }

    pub fn read8(&self, offset: u32) -> u8 {
        self.data[offset as usize]
    }

    pub fn read16(&self, offset: u32) -> u16 {
        let off = offset as usize;
        u16::from_le_bytes([self.data[off], self.data[off + 1]])
    }

    pub fn read32(&self, offset: u32) -> u32 {
        let off = offset as usize;
        u32::from_le_bytes([
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
        ])
    }

    pub fn write8(&mut self, offset: u32, val: u8) {
        self.data[offset as usize] = val;
    }

    pub fn write16(&mut self, offset: u32, val: u16) {
        let off = offset as usize;
        let bytes = val.to_le_bytes();
        self.data[off] = bytes[0];
        self.data[off + 1] = bytes[1];
    }

    pub fn write32(&mut self, offset: u32, val: u32) {
        let off = offset as usize;
        let bytes = val.to_le_bytes();
        self.data[off] = bytes[0];
        self.data[off + 1] = bytes[1];
        self.data[off + 2] = bytes[2];
        self.data[off + 3] = bytes[3];
    }

    /// 加载二进制数据到指定偏移
    pub fn load_binary(&mut self, offset: u32, data: &[u8]) {
        let off = offset as usize;
        self.data[off..off + data.len()].copy_from_slice(data);
    }
}

impl super::bus::Device for Memory {
    fn read8(&mut self, offset: u32) -> u8 { self.read8(offset) }
    fn read16(&mut self, offset: u32) -> u16 { self.read16(offset) }
    fn read32(&mut self, offset: u32) -> u32 { self.read32(offset) }
    fn write8(&mut self, offset: u32, val: u8) { self.write8(offset, val) }
    fn write16(&mut self, offset: u32, val: u16) { self.write16(offset, val) }
    fn write32(&mut self, offset: u32, val: u32) { self.write32(offset, val) }
}
