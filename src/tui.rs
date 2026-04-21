//! TUI 仪表盘模式：实时展示 CPU 内部状态
//!
//! 功能特性 `tui` 启用时可用，通过 `--tui` 参数激活。
//! 展示内容：寄存器组 / PC+反汇编 / CSR 状态 / UART 输出 / 运行统计

use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{ExecutableCommand, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::bus::Bus;
use crate::cpu::{self, Hart};

const FRAME_BUDGET: Duration = Duration::from_millis(16); // ~60 FPS
const ACTIVE_POLL_TIMEOUT: Duration = Duration::from_millis(5);
const IDLE_POLL_TIMEOUT: Duration = Duration::from_millis(50);
const MIDFRAME_EVENT_INTERVAL: u64 = 10_000;
const SCROLL_LINES: u16 = 3;
const UART_HISTORY_LIMIT: usize = 16_384;
const UART_HISTORY_RETAIN: usize = 12_288;

/// TUI 运行时状态
pub struct TuiState {
    uart_bytes: Vec<u8>,     // 原始 UART 字节流（UTF-8 积累缓冲）
    uart_text: String,       // 已确认的历史文本（不含当前行）
    current_line: Vec<char>, // 当前行内容（支持 \r 覆写）
    cursor_pos: usize,       // 当前行光标位置
    cycle: u64,
    max_cycles: u64,
    running: bool,
    finished: bool,
    finish_reason: Option<String>,
    step_request: u64,
    input_mode: bool,
    scroll_offset: u16,   // UART 窗口向上滚动的行数
    in_escape: bool,      // 正在解析 ANSI 转义序列
    escape_started: bool, // ESC 后已收到 [ 等起始符
    escape_buf: Vec<u8>,  // ANSI 序列参数缓冲（用于解析 [K 等）
}

impl TuiState {
    pub fn new(max_cycles: u64) -> Self {
        Self {
            uart_bytes: Vec::new(),
            uart_text: String::new(),
            current_line: Vec::new(),
            cursor_pos: 0,
            cycle: 0,
            max_cycles,
            running: true,
            finished: false,
            finish_reason: None,
            step_request: 0,
            input_mode: false,
            scroll_offset: 0,
            in_escape: false,
            escape_started: false,
            escape_buf: Vec::new(),
        }
    }

    /// 获取完整显示文本（历史 + 当前行）
    fn display_text(&self) -> String {
        let mut text = self.uart_text.clone();
        if !self.current_line.is_empty() {
            // 只显示到光标位置或最后一个非空字符（取较大值）
            // 避免退格后显示拖尾空格
            let content_end = self
                .current_line
                .iter()
                .rposition(|c| *c != ' ')
                .map(|i| i + 1)
                .unwrap_or(0);
            let visible_end = self.cursor_pos.max(content_end);
            text.extend(self.current_line[..visible_end].iter());
        }
        text
    }

    fn write_char_at_cursor(&mut self, ch: char) {
        if self.cursor_pos < self.current_line.len() {
            self.current_line[self.cursor_pos] = ch;
        } else {
            self.current_line.push(ch);
        }
        self.cursor_pos += 1;
    }

    fn write_text_at_cursor(&mut self, text: &str) {
        for ch in text.chars() {
            self.write_char_at_cursor(ch);
        }
    }

    fn commit_current_line(&mut self) {
        let line: String = self.current_line.iter().collect();
        self.uart_text.push_str(&line);
        self.uart_text.push('\n');
        self.current_line.clear();
        self.cursor_pos = 0;
    }

    fn trim_uart_history(&mut self) {
        if self.uart_text.len() > UART_HISTORY_LIMIT {
            let drain_to = self.uart_text.len() - UART_HISTORY_RETAIN;
            let drain_to = self.uart_text.ceil_char_boundary(drain_to);
            self.uart_text.drain(..drain_to);
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(SCROLL_LINES);
    }

    fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(SCROLL_LINES);
    }

    fn finish(&mut self, reason: impl Into<String>) {
        self.finished = true;
        self.finish_reason = Some(reason.into());
    }

    /// 添加 UART 字节，处理 VT100 基本控制序列
    fn push_uart_byte(&mut self, byte: u8) {
        // ANSI 转义序列处理
        if byte == 0x1b {
            self.in_escape = true;
            self.escape_started = false;
            self.escape_buf.clear();
            return;
        }
        if self.in_escape {
            if !self.escape_started {
                if byte == b'[' {
                    self.escape_started = true;
                    return;
                }
                // 非 CSI 序列（ESC + 单字符），忽略
                self.in_escape = false;
                return;
            }
            // CSI 序列参数字节 (0x30-0x3F) 和中间字节 (0x20-0x2F)
            if (0x20..=0x3F).contains(&byte) {
                self.escape_buf.push(byte);
                return;
            }
            // 终止字节 (0x40-0x7E) → 执行 CSI 命令
            if (0x40..=0x7E).contains(&byte) {
                self.handle_csi(byte);
                self.in_escape = false;
                return;
            }
            // 非法字节，放弃序列
            self.in_escape = false;
            return;
        }

        // 控制字符处理
        match byte {
            b'\n' => {
                // 换行：提交当前行
                self.commit_current_line();
            }
            b'\r' => {
                // 回车：光标移到行首（下一个字符从头覆写）
                self.cursor_pos = 0;
            }
            0x08 | 0x7f => {
                // 退格：光标左移一格
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            b'\t' => {
                // Tab：移到下一个 8 对齐位置
                let next_tab = (self.cursor_pos + 8) & !7;
                while self.cursor_pos < next_tab {
                    if self.cursor_pos < self.current_line.len() {
                        self.cursor_pos += 1;
                    } else {
                        self.current_line.push(' ');
                        self.cursor_pos += 1;
                    }
                }
            }
            _ if byte < 0x20 => {
                // 其他控制字符忽略
            }
            _ => {
                // 可打印字符或 UTF-8 多字节序列的一部分
                self.uart_bytes.push(byte);
                // 尝试 UTF-8 解码
                match std::str::from_utf8(&self.uart_bytes) {
                    Ok(s) => {
                        let decoded = s.to_string();
                        self.write_text_at_cursor(&decoded);
                        self.uart_bytes.clear();
                    }
                    Err(e) => {
                        let valid_up_to = e.valid_up_to();
                        if valid_up_to > 0 {
                            let valid =
                                std::str::from_utf8(&self.uart_bytes[..valid_up_to]).unwrap();
                            let decoded = valid.to_string();
                            self.write_text_at_cursor(&decoded);
                            self.uart_bytes.drain(..valid_up_to);
                        }
                        if self.uart_bytes.len() > 4 {
                            self.uart_bytes.remove(0);
                        }
                    }
                }
            }
        }

        // 限制总文本大小
        self.trim_uart_history();
    }

    /// 处理 CSI (ESC[) 命令
    fn handle_csi(&mut self, cmd: u8) {
        let param = std::str::from_utf8(&self.escape_buf)
            .unwrap_or("")
            .to_string();
        match cmd {
            b'K' => {
                // 擦除行：[0K 光标到行尾, [1K 行首到光标, [2K 整行
                let n: u8 = param.parse().unwrap_or(0);
                match n {
                    0 => self.current_line.truncate(self.cursor_pos),
                    1 => {
                        for i in 0..self.cursor_pos.min(self.current_line.len()) {
                            self.current_line[i] = ' ';
                        }
                    }
                    2 => {
                        self.current_line.clear();
                        self.cursor_pos = 0;
                    }
                    _ => {}
                }
            }
            b'C' => {
                // 光标右移 n 格
                let n: usize = param.parse().unwrap_or(1).max(1);
                self.cursor_pos = (self.cursor_pos + n).min(self.current_line.len());
            }
            b'D' => {
                // 光标左移 n 格
                let n: usize = param.parse().unwrap_or(1).max(1);
                self.cursor_pos = self.cursor_pos.saturating_sub(n);
            }
            b'G' => {
                // 光标移到第 n 列（1-based）
                let n: usize = param.parse().unwrap_or(1).max(1);
                self.cursor_pos = (n - 1).min(self.current_line.len());
            }
            b'J' => {
                // 擦除屏幕：[2J 清屏
                let n: u8 = param.parse().unwrap_or(0);
                if n == 2 {
                    self.uart_text.clear();
                    self.current_line.clear();
                    self.cursor_pos = 0;
                }
            }
            b'P' => {
                // 删除字符：从光标位置删除 n 个字符（后面的左移）
                let n: usize = param.parse().unwrap_or(1).max(1);
                let end = (self.cursor_pos + n).min(self.current_line.len());
                self.current_line.drain(self.cursor_pos..end);
            }
            b'@' => {
                // 插入空白：在光标位置插入 n 个空格
                let n: usize = param.parse().unwrap_or(1).max(1);
                for _ in 0..n {
                    if self.cursor_pos <= self.current_line.len() {
                        self.current_line.insert(self.cursor_pos, ' ');
                    }
                }
            }
            _ => {} // 忽略其他 CSI 命令（颜色等）
        }
    }
}

fn poll_timeout(state: &TuiState) -> Duration {
    if state.running && !state.finished {
        ACTIVE_POLL_TIMEOUT
    } else {
        IDLE_POLL_TIMEOUT
    }
}

fn drain_uart_output(state: &mut TuiState, bus: &mut Bus) {
    while let Some(ch) = bus.uart.pop_output() {
        state.push_uart_byte(ch);
    }
}

fn finish_normal_from_tohost(state: &mut TuiState, val: u32) {
    if val == 1 {
        state.finish("PASS");
    } else {
        state.finish(format!("FAIL (test case {})", val >> 1));
    }
}

/// 初始化终端
fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// 恢复终端
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    terminal::disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// TUI 主循环入口（Linux 模式）
pub fn run_tui_linux(hart: &mut Hart, bus: &mut Bus, max_cycles: u64) {
    let mut terminal = setup_terminal().expect("无法初始化终端");
    let mut state = TuiState::new(max_cycles);

    loop {
        // 渲染
        terminal
            .draw(|frame| draw_dashboard(frame, hart, bus, &state))
            .expect("渲染失败");

        // 处理输入事件（drain 所有 pending 事件）
        while event::poll(poll_timeout(&state)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match handle_key(key, &state) {
                    KeyAction::Quit => {
                        restore_terminal(&mut terminal).expect("无法恢复终端");
                        if let Some(reason) = &state.finish_reason {
                            println!("TUI 结束：{} (cycle={})", reason, state.cycle);
                        }
                        return;
                    }
                    KeyAction::TogglePause => state.running = !state.running,
                    KeyAction::ToggleInputMode => state.input_mode = !state.input_mode,
                    KeyAction::Step(n) => state.step_request = n,
                    KeyAction::Input(byte) => bus.uart.push_input(byte),
                    KeyAction::ScrollUp => state.scroll_up(),
                    KeyAction::ScrollDown => state.scroll_down(),
                    KeyAction::None => {}
                }
            } else {
                break;
            }
        }

        if state.finished {
            // 结束后等待用户按 q 退出
            continue;
        }

        // 单步模式：暂停时执行请求的步数
        if state.step_request > 0 && !state.running {
            let steps = state.step_request;
            state.step_request = 0;
            for _ in 0..steps {
                if state.cycle >= state.max_cycles {
                    state.finish("达到最大周期数");
                    break;
                }
                hart.step(bus);
                state.cycle += 1;
                drain_uart_output(&mut state, bus);
                if hart.shutdown_requested() {
                    state.finish("SBI shutdown");
                    break;
                }
                if hart.pc == 0 {
                    state.finish("Fatal: PC=0");
                    break;
                }
            }
            continue;
        }

        if !state.running {
            continue;
        }

        // 执行 CPU 步进（用帧时间预算控制，自适应 debug/release 速度）
        let frame_start = Instant::now();
        let mut steps_this_frame = 0u64;

        loop {
            if state.cycle >= state.max_cycles {
                state.finish("达到最大周期数");
                break;
            }

            hart.step(bus);
            state.cycle += 1;
            steps_this_frame += 1;

            drain_uart_output(&mut state, bus);

            if hart.shutdown_requested() {
                state.finish("SBI shutdown");
                break;
            }

            if hart.pc == 0 {
                state.finish("Fatal: PC=0");
                break;
            }

            // 每隔固定步数检查一次按键，提高运行态响应速度
            if steps_this_frame % MIDFRAME_EVENT_INTERVAL == 0 {
                if event::poll(Duration::ZERO).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = event::read() {
                        match handle_key(key, &state) {
                            KeyAction::Quit => {
                                restore_terminal(&mut terminal).expect("无法恢复终端");
                                return;
                            }
                            KeyAction::TogglePause => {
                                state.running = false;
                                break;
                            }
                            KeyAction::ToggleInputMode => state.input_mode = !state.input_mode,
                            KeyAction::Input(byte) => bus.uart.push_input(byte),
                            KeyAction::ScrollUp => state.scroll_up(),
                            KeyAction::ScrollDown => state.scroll_down(),
                            _ => {}
                        }
                    }
                }
                // 帧时间预算用完则让出渲染
                if frame_start.elapsed() >= FRAME_BUDGET {
                    break;
                }
            }
        }
    }
}

/// TUI 主循环入口（普通模式）
pub fn run_tui_normal(hart: &mut Hart, bus: &mut Bus, max_cycles: u64) {
    let mut terminal = setup_terminal().expect("无法初始化终端");
    let mut state = TuiState::new(max_cycles);

    loop {
        terminal
            .draw(|frame| draw_dashboard(frame, hart, bus, &state))
            .expect("渲染失败");

        // 处理输入事件（drain 所有 pending 事件）
        while event::poll(poll_timeout(&state)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match handle_key(key, &state) {
                    KeyAction::Quit => {
                        restore_terminal(&mut terminal).expect("无法恢复终端");
                        if let Some(reason) = &state.finish_reason {
                            println!("TUI 结束：{} (cycle={})", reason, state.cycle);
                        }
                        return;
                    }
                    KeyAction::TogglePause => state.running = !state.running,
                    KeyAction::ToggleInputMode => state.input_mode = !state.input_mode,
                    KeyAction::Step(n) => state.step_request = n,
                    KeyAction::Input(byte) => bus.uart.push_input(byte),
                    KeyAction::ScrollUp => state.scroll_up(),
                    KeyAction::ScrollDown => state.scroll_down(),
                    KeyAction::None => {}
                }
            } else {
                break;
            }
        }

        if state.finished {
            continue;
        }

        // 单步模式
        if state.step_request > 0 && !state.running {
            let steps = state.step_request;
            state.step_request = 0;
            for _ in 0..steps {
                if state.cycle >= state.max_cycles {
                    state.finish("Timeout");
                    break;
                }
                hart.step(bus);
                state.cycle += 1;
                drain_uart_output(&mut state, bus);
                if let Some(val) = bus.tohost_value {
                    finish_normal_from_tohost(&mut state, val);
                    break;
                }
            }
            continue;
        }

        if !state.running {
            continue;
        }

        let frame_start = Instant::now();
        let mut steps_this_frame = 0u64;
        loop {
            if state.cycle >= state.max_cycles {
                state.finish("Timeout");
                break;
            }

            hart.step(bus);
            state.cycle += 1;
            steps_this_frame += 1;

            drain_uart_output(&mut state, bus);

            if let Some(val) = bus.tohost_value {
                finish_normal_from_tohost(&mut state, val);
                break;
            }

            if hart.pc == 0 {
                state.finish("Fatal: PC=0");
                break;
            }

            // 每隔固定步数检查一次按键，避免 release 模式下输入滞后
            if steps_this_frame % MIDFRAME_EVENT_INTERVAL == 0 {
                if event::poll(Duration::ZERO).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = event::read() {
                        match handle_key(key, &state) {
                            KeyAction::Quit => {
                                restore_terminal(&mut terminal).expect("无法恢复终端");
                                return;
                            }
                            KeyAction::TogglePause => {
                                state.running = false;
                                break;
                            }
                            KeyAction::ToggleInputMode => state.input_mode = !state.input_mode,
                            KeyAction::Input(byte) => bus.uart.push_input(byte),
                            KeyAction::ScrollUp => state.scroll_up(),
                            KeyAction::ScrollDown => state.scroll_down(),
                            _ => {}
                        }
                    }
                }
                if frame_start.elapsed() >= FRAME_BUDGET {
                    break;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyAction {
    Quit,
    TogglePause,
    ToggleInputMode,
    Step(u64),
    Input(u8),
    ScrollUp,
    ScrollDown,
    None,
}

fn handle_key(key: KeyEvent, state: &TuiState) -> KeyAction {
    // Windows 下 crossterm 会同时发 Press + Release 事件，只处理 Press
    if key.kind != KeyEventKind::Press {
        return KeyAction::None;
    }

    // F1 切换输入模式（运行中时可用，最高优先级）
    if key.code == KeyCode::F(1) && state.running && !state.finished {
        return KeyAction::ToggleInputMode;
    }

    // 输入模式：除 F1/Esc 外所有按键转发 UART（包括 Ctrl+C → 0x03）
    if state.input_mode && state.running && !state.finished {
        match key.code {
            KeyCode::Esc => return KeyAction::ToggleInputMode,
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    let ctrl_byte = (c as u8).wrapping_sub(b'a' - 1);
                    return KeyAction::Input(ctrl_byte);
                }
                return KeyAction::Input(c as u8);
            }
            KeyCode::Enter => return KeyAction::Input(b'\n'),
            KeyCode::Tab => return KeyAction::Input(b'\t'),
            KeyCode::Backspace => return KeyAction::Input(0x7f),
            _ => return KeyAction::None,
        }
    }

    // 非输入模式下：Ctrl+C 退出
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return KeyAction::Quit;
    }

    // Esc：退出程序
    if key.code == KeyCode::Esc {
        return KeyAction::Quit;
    }

    // 已结束
    if state.finished {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char(' ') => return KeyAction::Quit,
            KeyCode::Up | KeyCode::PageUp => return KeyAction::ScrollUp,
            KeyCode::Down | KeyCode::PageDown => return KeyAction::ScrollDown,
            _ => return KeyAction::None,
        }
    }

    // 暂停状态下的控制键
    if !state.running {
        match key.code {
            KeyCode::Char('q') => return KeyAction::Quit,
            KeyCode::Char(' ') => return KeyAction::TogglePause,
            KeyCode::Char('n') => return KeyAction::Step(1),
            KeyCode::Char('N') => return KeyAction::Step(10),
            KeyCode::Char('m') => return KeyAction::Step(100),
            KeyCode::Char('M') => return KeyAction::Step(1000),
            KeyCode::Up | KeyCode::PageUp => return KeyAction::ScrollUp,
            KeyCode::Down | KeyCode::PageDown => return KeyAction::ScrollDown,
            _ => return KeyAction::None,
        }
    }

    // 运行中 + 控制模式
    match key.code {
        KeyCode::Char(' ') => KeyAction::TogglePause,
        KeyCode::Up | KeyCode::PageUp => KeyAction::ScrollUp,
        KeyCode::Down | KeyCode::PageDown => KeyAction::ScrollDown,
        _ => KeyAction::None,
    }
}

/// 绘制仪表盘
fn draw_dashboard(frame: &mut ratatui::Frame, hart: &Hart, bus: &Bus, state: &TuiState) {
    let area = frame.area();

    // 主布局：上半部分（寄存器+状态）和下半部分（UART）+ 状态栏
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30), // 上半：寄存器 + CSR
            Constraint::Min(5),         // 下半：UART 输出（填充剩余空间）
            Constraint::Length(1),      // 底部状态栏（始终 1 行）
        ])
        .split(area);

    // 上半：左（寄存器）+ 右（CSR/状态）
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(main_chunks[0]);

    draw_registers(frame, hart, top_chunks[0]);
    draw_status(frame, hart, bus, state, top_chunks[1]);
    draw_uart(frame, state, main_chunks[1]);
    draw_statusbar(frame, state, main_chunks[2]);
}

/// 寄存器面板
fn draw_registers(frame: &mut ratatui::Frame, hart: &Hart, area: Rect) {
    let reg_names = [
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
        "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
        "t5", "t6",
    ];

    let mut lines = Vec::new();
    for row in 0..8 {
        let mut spans = Vec::new();
        for col in 0..4 {
            let idx = row * 4 + col;
            let val = hart.read_reg(idx);
            let name = reg_names[idx];
            let text = format!("{:4}={:08x} ", name, val);
            let style = if val != 0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }

    let block = Block::default()
        .title(" 寄存器 (x0-x31) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// 状态/CSR 面板
fn draw_status(frame: &mut ratatui::Frame, hart: &Hart, _bus: &Bus, state: &TuiState, area: Rect) {
    let priv_str = match hart.privilege {
        0 => "U (User)",
        1 => "S (Supervisor)",
        3 => "M (Machine)",
        _ => "?",
    };
    let priv_color = match hart.privilege {
        0 => Color::Green,
        1 => Color::Yellow,
        3 => Color::Red,
        _ => Color::White,
    };

    let mstatus = hart.read_csr(cpu::MSTATUS);
    let mcause = hart.read_csr(cpu::MCAUSE);
    let mepc = hart.read_csr(cpu::MEPC);
    let scause = hart.read_csr(cpu::SCAUSE);
    let sepc = hart.read_csr(cpu::SEPC);
    let satp = hart.read_csr(cpu::SATP);
    let mip = hart.read_csr(cpu::MIP);

    let lines = vec![
        Line::from(vec![
            Span::raw("  PC:    "),
            Span::styled(
                format!("0x{:08x}", hart.pc),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Mode:  "),
            Span::styled(priv_str, Style::default().fg(priv_color)),
        ]),
        Line::from(format!("  Cycle: {}", state.cycle)),
        Line::from(""),
        Line::from(format!("  mstatus: 0x{:08x}", mstatus)),
        Line::from(format!("  mcause:  0x{:08x}  mepc: 0x{:08x}", mcause, mepc)),
        Line::from(format!("  scause:  0x{:08x}  sepc: 0x{:08x}", scause, sepc)),
        Line::from(format!("  satp:    0x{:08x}  mip:  0x{:08x}", satp, mip)),
        Line::from(""),
        Line::from(format!(
            "  mideleg: 0x{:08x}  medeleg: 0x{:08x}",
            hart.read_csr(cpu::MIDELEG),
            hart.read_csr(cpu::MEDELEG)
        )),
    ];

    let block = Block::default()
        .title(" CPU 状态 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// UART 输出面板（支持滚动）
fn draw_uart(frame: &mut ratatui::Frame, state: &TuiState, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let display = state.display_text();
    let all_lines: Vec<&str> = display.lines().collect();
    let total = all_lines.len();
    // 限制滚动范围：最多滚到只显示第一行
    let max_scroll = total.saturating_sub(inner_height);
    let scroll = (state.scroll_offset as usize).min(max_scroll);

    let scroll_hint = if scroll > 0 {
        format!(" UART 输出 [↑{}行] ", scroll)
    } else {
        " UART 输出 ".to_string()
    };
    let block = Block::default()
        .title(scroll_hint)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    // 计算显示窗口：从末尾向上偏移 scroll 行
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);
    let visible: Vec<Line> = all_lines[start..end]
        .iter()
        .map(|l| Line::from(l.to_string()))
        .collect();

    let paragraph = Paragraph::new(visible)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// 底部状态栏
fn draw_statusbar(frame: &mut ratatui::Frame, state: &TuiState, area: Rect) {
    let status = if state.finished {
        let reason = state.finish_reason.as_deref().unwrap_or("done");
        format!(" ■ {} | q/Esc 退出", reason)
    } else if state.input_mode {
        " ⌨ 输入模式 | 键盘→UART(含Ctrl+C) | F1/Esc 切回控制".to_string()
    } else if state.running {
        " ▶ 运行中 | Space 暂停 | F1 输入模式 | Esc/Ctrl+C 退出".to_string()
    } else {
        " ⏸ 已暂停 | Space 继续 | F1 输入模式 | n/N/m/M=1/10/100/1000步 | q/Esc 退出".to_string()
    };

    let style = if state.finished {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else if state.running {
        Style::default().bg(Color::Blue).fg(Color::White)
    } else {
        Style::default().bg(Color::Yellow).fg(Color::Black)
    };

    let bar = Paragraph::new(status).style(style);
    frame.render_widget(bar, area);
}
