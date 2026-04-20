use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// UART 16550 极简实现
///
/// 支持：
/// - THR 输出（写入时打印到主机终端）
/// - RBR 输入（从主机 stdin 异步拉取）
/// - IER 接收中断使能（bit0）
/// - LSR 状态位（DR/TEMT/THRE）
pub struct Uart {
    rx_fifo: VecDeque<u8>,
    tx_buffer: VecDeque<u8>,
    capture_output: bool,
    ier: u8,
    lcr: u8,
    mcr: u8,
    scr: u8,
    dll: u8,
    dlm: u8,
    fcr: u8,
    rx_irq_pending: bool,
    tx_irq_pending: bool,
    host_rx: Option<Receiver<u8>>,
}

impl Uart {
    pub fn new() -> Self {
        Uart {
            rx_fifo: VecDeque::new(),
            tx_buffer: VecDeque::new(),
            capture_output: false,
            ier: 0,
            lcr: 0,
            mcr: 0,
            scr: 0,
            dll: 0,
            dlm: 0,
            fcr: 0,
            rx_irq_pending: false,
            tx_irq_pending: false,
            host_rx: None,
        }
    }

    fn dlab(&self) -> bool {
        (self.lcr & 0x80) != 0
    }

    fn interrupt_pending(&self) -> bool {
        (self.rx_irq_pending && (self.ier & 0x01) != 0)
            || (self.tx_irq_pending && (self.ier & 0x02) != 0)
    }

    /// Linux 模式启用主机输入桥接：后台线程读取 stdin 字节并投递到 UART RX FIFO。
    pub fn enable_host_input(&mut self) {
        if self.host_rx.is_some() {
            return;
        }

        let (tx, rx) = mpsc::channel::<u8>();
        self.host_rx = Some(rx);

        thread::spawn(move || {
            let stdin = io::stdin();
            let mut locked = stdin.lock();
            let mut byte = [0u8; 1];
            loop {
                match locked.read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(byte[0]).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    /// 从 host 输入通道拉取数据并返回是否应触发 RX 中断。
    pub fn poll_host_input(&mut self) -> bool {
        if let Some(ref rx) = self.host_rx {
            while let Ok(ch) = rx.try_recv() {
                self.rx_fifo.push_back(ch);
                if (self.ier & 0x01) != 0 {
                    self.rx_irq_pending = true;
                }
            }
        }
        self.interrupt_pending()
    }

    /// 启用输出捕获模式（TUI 用）：输出不打印到 stdout，而是存入缓冲区。
    #[allow(dead_code)]
    pub fn enable_capture(&mut self) {
        self.capture_output = true;
    }

    /// 从输出缓冲区弹出一个字节（TUI 轮询用）。
    #[allow(dead_code)]
    pub fn pop_output(&mut self) -> Option<u8> {
        self.tx_buffer.pop_front()
    }

    /// 向 RX FIFO 注入一个字节（TUI 键盘输入用）。
    #[allow(dead_code)]
    pub fn push_input(&mut self, byte: u8) {
        self.rx_fifo.push_back(byte);
        if (self.ier & 0x01) != 0 {
            self.rx_irq_pending = true;
        }
    }
}

// 寄存器偏移
const RBR: u32 = 0x00; // Receive Buffer Register (只读)
const THR: u32 = 0x00; // Transmit Holding Register (只写)
const IER: u32 = 0x01; // Interrupt Enable Register
const IIR: u32 = 0x02; // Interrupt Identification Register
const FCR: u32 = 0x02; // FIFO Control Register
const LCR: u32 = 0x03; // Line Control Register
const MCR: u32 = 0x04; // Modem Control Register
const LSR: u32 = 0x05; // Line Status Register
const MSR: u32 = 0x06; // Modem Status Register
const SCR: u32 = 0x07; // Scratch Register

impl super::bus::Device for Uart {
    fn read8(&mut self, offset: u32) -> u8 {
        match offset {
            RBR => {
                if self.dlab() {
                    self.dll
                } else {
                    let v = self.rx_fifo.pop_front().unwrap_or(0);
                    if self.rx_fifo.is_empty() {
                        self.rx_irq_pending = false;
                    }
                    v
                }
            }
            IER => {
                if self.dlab() {
                    self.dlm
                } else {
                    self.ier
                }
            }
            IIR => {
                if self.rx_irq_pending && (self.ier & 0x01) != 0 {
                    0x04 // Received Data Available
                } else if self.tx_irq_pending && (self.ier & 0x02) != 0 {
                    self.tx_irq_pending = false;
                    0x02 // THR Empty
                } else {
                    0x01 // No interrupt pending
                }
            }
            LCR => self.lcr,
            MCR => self.mcr,
            // bit0=DR(有接收数据), bit5=THR empty, bit6=transmitter idle
            LSR => 0x60 | if self.rx_fifo.is_empty() { 0 } else { 0x01 },
            MSR => 0xB0, // DCD/DSR/CTS asserted, minimal modem status
            SCR => self.scr,
            _ => 0,
        }
    }

    fn write8(&mut self, offset: u32, val: u8) {
        match offset {
            THR => {
                if self.dlab() {
                    self.dll = val;
                } else {
                    if self.capture_output {
                        self.tx_buffer.push_back(val);
                    } else {
                        print!("{}", val as char);
                        let _ = io::stdout().flush();
                    }
                    if (self.ier & 0x02) != 0 {
                        self.tx_irq_pending = true;
                    }
                }
            }
            IER => {
                if self.dlab() {
                    self.dlm = val;
                } else {
                    let old = self.ier;
                    self.ier = val & 0x0F;
                    if (old & 0x02) == 0 && (self.ier & 0x02) != 0 {
                        // 刚开启 THRE 中断，立即报告一次空发送缓冲可写。
                        self.tx_irq_pending = true;
                    }
                    if !self.rx_fifo.is_empty() && (self.ier & 0x01) != 0 {
                        self.rx_irq_pending = true;
                    }
                }
            }
            FCR => {
                self.fcr = val;
                if (val & 0x02) != 0 {
                    self.rx_fifo.clear();
                    self.rx_irq_pending = false;
                }
                if (val & 0x04) != 0 {
                    self.tx_irq_pending = false;
                }
            }
            LCR => self.lcr = val,
            MCR => self.mcr = val,
            SCR => self.scr = val,
            _ => {}
        }
    }
}
