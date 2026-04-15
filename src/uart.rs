/// UART 16550 极简实现
///
/// 只实现输出功能：THR 写入直接打印到 host stdout
/// LSR 常返回 0x60（TX empty + TX idle）
pub struct Uart;

impl Uart {
    pub fn new() -> Self {
        Uart
    }
}

// 寄存器偏移
const THR: u32 = 0x00; // Transmit Holding Register (只写)
const LSR: u32 = 0x05; // Line Status Register (只读)

impl super::bus::Device for Uart {
    fn read8(&self, offset: u32) -> u8 {
        match offset {
            LSR => 0x60, // bit5=THR empty, bit6=transmitter idle
            _ => 0,
        }
    }

    fn write8(&mut self, offset: u32, val: u8) {
        if offset == THR {
            print!("{}", val as char);
        }
    }
}
