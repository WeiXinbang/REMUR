/// PLIC (Platform-Level Interrupt Controller) 极简实现
///
/// 只支持 1 个中断源 (UART, IRQ#10), 1 个 context (S-mode Hart 0)
///
/// 寄存器布局 (相对偏移):
///   0x000028: source 10 priority   (4 bytes)
///   0x002080: context 0 enable     (4 bytes, bit10 = UART)
///   0x200000: context 0 threshold  (4 bytes)
///   0x200004: context 0 claim/complete (4 bytes)
pub struct Plic {
    priority: [u32; 1024],   // source priorities
    pending: u32,            // pending bits (bit per source)
    enable: u32,             // context 0 enable bits
    threshold: u32,          // context 0 threshold
    claimed: u32,            // currently claimed IRQs (bit per source)
}

impl Plic {
    pub fn new() -> Self {
        Plic {
            priority: [0; 1024],
            pending: 0,
            enable: 0,
            threshold: 0,
            claimed: 0,
        }
    }

    /// 外部设备触发中断
    pub fn set_pending(&mut self, irq: u32) {
        if irq > 0 && irq < 1024 {
            self.pending |= 1 << irq;
        }
    }

    /// 检查是否有待处理的外部中断
    pub fn has_pending_interrupt(&self) -> bool {
        let active = self.pending & self.enable;
        // 检查是否有任何 enabled + pending 的源其优先级 > threshold
        for irq in 1..32u32 {
            if (active >> irq) & 1 != 0 && self.priority[irq as usize] > self.threshold {
                return true;
            }
        }
        false
    }
}

impl super::bus::Device for Plic {
    fn read32(&mut self, offset: u32) -> u32 {
        match offset {
            // source priorities: 0x000000..0x001000
            0x000000..=0x000FFF => {
                let src = offset / 4;
                if (src as usize) < self.priority.len() {
                    self.priority[src as usize]
                } else {
                    0
                }
            }
            // pending bits
            0x001000 => self.pending,
            // context 0 enable (offset 0x002000 + ctx*0x80)
            0x002000..=0x00207F => {
                if offset == 0x002000 {
                    self.enable
                } else {
                    0
                }
            }
            // context 0 threshold
            0x200000 => self.threshold,
            // context 0 claim — 返回最高优先级待处理 IRQ 并原子清除 pending
            0x200004 => {
                let active = self.pending & self.enable;
                let mut best_irq = 0u32;
                let mut best_prio = 0u32;
                for irq in 1..32u32 {
                    if (active >> irq) & 1 != 0 {
                        let prio = self.priority[irq as usize];
                        if prio > self.threshold && prio > best_prio {
                            best_prio = prio;
                            best_irq = irq;
                        }
                    }
                }
                if best_irq != 0 {
                    self.pending &= !(1 << best_irq);
                    self.claimed |= 1 << best_irq;
                }
                best_irq
            }
            _ => 0,
        }
    }

    fn write32(&mut self, offset: u32, val: u32) {
        match offset {
            // source priorities
            0x000000..=0x000FFF => {
                let src = offset / 4;
                if (src as usize) < self.priority.len() {
                    self.priority[src as usize] = val;
                }
            }
            // context 0 enable
            0x002000..=0x00207F => {
                if offset == 0x002000 {
                    self.enable = val;
                }
            }
            // context 0 threshold
            0x200000 => self.threshold = val,
            // context 0 complete — 清除 claimed 位，允许再次触发
            0x200004 => {
                if val > 0 && val < 32 {
                    self.claimed &= !(1 << val);
                }
            }
            _ => {}
        }
    }
}
