/// PLIC (Platform-Level Interrupt Controller) 极简实现
///
/// 支持 31 个中断源（仅实现首个 pending/enable word）与 2 个 context：
/// - context 0: M-mode hart0
/// - context 1: S-mode hart0
///
/// 关键寄存器布局（相对偏移）：
/// - priority: 0x000000 + irq*4
/// - pending:  0x001000
/// - enable:   0x002000 + ctx*0x80
/// - context:  0x200000 + ctx*0x1000
///   - threshold: +0x0
///   - claim/complete: +0x4
pub struct Plic {
    priority: [u32; 1024],
    pending: u32,
    enable: [u32; 2],
    threshold: [u32; 2],
    claimed: [u32; 2],
}

impl Plic {
    pub fn new() -> Self {
        Self {
            priority: [0; 1024],
            pending: 0,
            enable: [0; 2],
            threshold: [0; 2],
            claimed: [0; 2],
        }
    }

    /// 外部设备触发中断
    pub fn set_pending(&mut self, irq: u32) {
        if irq > 0 && irq < 32 {
            self.pending |= 1 << irq;
        }
    }

    fn best_irq_for_context(&self, ctx: usize) -> u32 {
        if ctx >= 2 {
            return 0;
        }
        let active = self.pending & self.enable[ctx] & !self.claimed[ctx];
        let mut best_irq = 0u32;
        let mut best_prio = 0u32;
        for irq in 1..32u32 {
            if (active >> irq) & 1 != 0 {
                let prio = self.priority[irq as usize];
                if prio > self.threshold[ctx] && prio > best_prio {
                    best_prio = prio;
                    best_irq = irq;
                }
            }
        }
        best_irq
    }

    fn claim(&mut self, ctx: usize) -> u32 {
        let best_irq = self.best_irq_for_context(ctx);
        if best_irq != 0 {
            self.pending &= !(1 << best_irq);
            self.claimed[ctx] |= 1 << best_irq;
        }
        best_irq
    }

    pub fn has_pending_m_interrupt(&self) -> bool {
        self.best_irq_for_context(0) != 0
    }

    pub fn has_pending_s_interrupt(&self) -> bool {
        self.best_irq_for_context(1) != 0
    }
}

impl super::bus::Device for Plic {
    fn read32(&mut self, offset: u32) -> u32 {
        match offset {
            // source priorities
            0x000000..=0x000FFF => {
                let src = offset / 4;
                if (src as usize) < self.priority.len() {
                    self.priority[src as usize]
                } else {
                    0
                }
            }
            // pending bits word0
            0x001000 => self.pending,
            // context enable word0 (0x2000 + ctx*0x80)
            0x002000..=0x0020FF => {
                let rel = offset - 0x002000;
                let ctx = (rel / 0x80) as usize;
                let word_off = rel % 0x80;
                if ctx < 2 && word_off == 0 {
                    self.enable[ctx]
                } else {
                    0
                }
            }
            // context threshold/claim (0x200000 + ctx*0x1000)
            0x200000..=0x201FFF => {
                let rel = offset - 0x200000;
                let ctx = (rel / 0x1000) as usize;
                let reg = rel % 0x1000;
                if ctx >= 2 {
                    return 0;
                }
                match reg {
                    0x0 => self.threshold[ctx],
                    0x4 => self.claim(ctx),
                    _ => 0,
                }
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
            // context enable word0
            0x002000..=0x0020FF => {
                let rel = offset - 0x002000;
                let ctx = (rel / 0x80) as usize;
                let word_off = rel % 0x80;
                if ctx < 2 && word_off == 0 {
                    self.enable[ctx] = val;
                }
            }
            // context threshold/complete
            0x200000..=0x201FFF => {
                let rel = offset - 0x200000;
                let ctx = (rel / 0x1000) as usize;
                let reg = rel % 0x1000;
                if ctx >= 2 {
                    return;
                }
                match reg {
                    0x0 => self.threshold[ctx] = val,
                    0x4 => {
                        if val > 0 && val < 32 {
                            self.claimed[ctx] &= !(1 << val);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
