/// CLINT (Core Local Interruptor)
///
/// 地址空间 (相对偏移):
///   0x0000: msip[0]     (4 bytes) - 软件中断挂起
///   0x4000: mtimecmp[0] (8 bytes) - 定时器比较值
///   0xBFF8: mtime       (8 bytes) - 实时计数器
pub struct Clint {
    pub msip: u32,
    pub mtimecmp: u64,
    pub mtime: u64,
}

impl Clint {
    pub fn new() -> Self {
        Clint {
            msip: 0,
            mtimecmp: u64::MAX, // 初始不触发
            mtime: 0,
        }
    }

    /// 每个 CPU 周期调用，递增 mtime
    pub fn tick(&mut self) {
        self.mtime = self.mtime.wrapping_add(1);
    }

    /// 检查是否应触发定时器中断
    pub fn timer_interrupt_pending(&self) -> bool {
        self.mtime >= self.mtimecmp
    }

    /// 检查是否有软件中断挂起
    pub fn software_interrupt_pending(&self) -> bool {
        (self.msip & 1) != 0
    }
}

impl super::bus::Device for Clint {
    fn read32(&mut self, offset: u32) -> u32 {
        match offset {
            0x0000 => self.msip,
            // mtimecmp low
            0x4000 => self.mtimecmp as u32,
            // mtimecmp high
            0x4004 => (self.mtimecmp >> 32) as u32,
            // mtime low
            0xBFF8 => self.mtime as u32,
            // mtime high
            0xBFFC => (self.mtime >> 32) as u32,
            _ => 0,
        }
    }

    fn write32(&mut self, offset: u32, val: u32) {
        match offset {
            0x0000 => self.msip = val & 1,
            // mtimecmp low
            0x4000 => {
                self.mtimecmp = (self.mtimecmp & 0xFFFF_FFFF_0000_0000) | val as u64;
            }
            // mtimecmp high
            0x4004 => {
                self.mtimecmp = (self.mtimecmp & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
            }
            // mtime low
            0xBFF8 => {
                self.mtime = (self.mtime & 0xFFFF_FFFF_0000_0000) | val as u64;
            }
            // mtime high
            0xBFFC => {
                self.mtime = (self.mtime & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
            }
            _ => {}
        }
    }
}
