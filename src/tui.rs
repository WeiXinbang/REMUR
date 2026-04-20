//! TUI 仪表盘模式：实时展示 CPU 内部状态
//!
//! 功能特性 `tui` 启用时可用，通过 `--tui` 参数激活。
//! 展示内容：寄存器组 / PC+反汇编 / CSR 状态 / UART 输出 / 运行统计

use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, ExecutableCommand};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;

use crate::bus::Bus;
use crate::cpu::{self, Hart};

const FRAME_BUDGET: Duration = Duration::from_millis(16); // ~60 FPS
const CYCLES_PER_FRAME: u64 = 50_000; // 每帧最多执行的指令数

/// TUI 运行时状态
pub struct TuiState {
    uart_buffer: String,
    cycle: u64,
    max_cycles: u64,
    running: bool,
    finished: bool,
    finish_reason: Option<String>,
}

impl TuiState {
    pub fn new(max_cycles: u64) -> Self {
        Self {
            uart_buffer: String::new(),
            cycle: 0,
            max_cycles,
            running: true,
            finished: false,
            finish_reason: None,
        }
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

        // 处理输入事件
        if event::poll(Duration::from_millis(1)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match handle_key(key, &mut state) {
                    KeyAction::Quit => break,
                    KeyAction::TogglePause => state.running = !state.running,
                    KeyAction::None => {}
                }
            }
        }

        if state.finished {
            // 结束后等待用户按 q 退出
            continue;
        }

        if !state.running {
            continue;
        }

        // 执行 CPU 步进
        let frame_start = Instant::now();
        let mut steps_this_frame = 0u64;

        while steps_this_frame < CYCLES_PER_FRAME {
            if state.cycle >= state.max_cycles {
                state.finished = true;
                state.finish_reason = Some("达到最大周期数".to_string());
                break;
            }

            hart.step(bus);
            state.cycle += 1;
            steps_this_frame += 1;

            // 收集 UART 输出（每步可能有多个字节）
            while let Some(ch) = bus.uart.pop_output() {
                if ch != b'\r' {
                    state.uart_buffer.push(ch as char);
                }
            }
            // 限制 buffer 大小
            if state.uart_buffer.len() > 8192 {
                let drain_to = state.uart_buffer.len() - 6144;
                state.uart_buffer.drain(..drain_to);
            }

            if hart.shutdown_requested() {
                state.finished = true;
                state.finish_reason = Some("SBI shutdown".to_string());
                break;
            }

            if hart.pc == 0 {
                state.finished = true;
                state.finish_reason = Some("Fatal: PC=0".to_string());
                break;
            }

            // 帧时间预算用完则让出渲染
            if frame_start.elapsed() >= FRAME_BUDGET {
                break;
            }
        }
    }

    restore_terminal(&mut terminal).expect("无法恢复终端");

    // 退出后打印最终状态
    if let Some(reason) = &state.finish_reason {
        println!("TUI 结束：{} (cycle={})", reason, state.cycle);
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

        if event::poll(Duration::from_millis(1)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match handle_key(key, &mut state) {
                    KeyAction::Quit => break,
                    KeyAction::TogglePause => state.running = !state.running,
                    KeyAction::None => {}
                }
            }
        }

        if state.finished || !state.running {
            continue;
        }

        let frame_start = Instant::now();
        while state.cycle < state.max_cycles {
            hart.step(bus);
            state.cycle += 1;

            if let Some(val) = bus.tohost_value {
                state.finished = true;
                if val == 1 {
                    state.finish_reason = Some("PASS".to_string());
                } else {
                    state.finish_reason = Some(format!("FAIL (test case {})", val >> 1));
                }
                break;
            }

            if frame_start.elapsed() >= FRAME_BUDGET {
                break;
            }
        }

        if state.cycle >= state.max_cycles && !state.finished {
            state.finished = true;
            state.finish_reason = Some("Timeout".to_string());
        }
    }

    restore_terminal(&mut terminal).expect("无法恢复终端");
    if let Some(reason) = &state.finish_reason {
        println!("TUI 结束：{} (cycle={})", reason, state.cycle);
    }
}

enum KeyAction {
    Quit,
    TogglePause,
    None,
}

fn handle_key(key: KeyEvent, _state: &mut TuiState) -> KeyAction {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => KeyAction::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
        KeyCode::Char(' ') => KeyAction::TogglePause,
        _ => KeyAction::None,
    }
}

/// 绘制仪表盘
fn draw_dashboard(
    frame: &mut ratatui::Frame,
    hart: &Hart,
    bus: &Bus,
    state: &TuiState,
) {
    let area = frame.area();

    // 主布局：上半部分（寄存器+状态）和下半部分（UART）
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(14),       // 上半：寄存器 + CSR
            Constraint::Percentage(40), // 下半：UART 输出
            Constraint::Length(1),      // 底部状态栏
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
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2",
        "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5",
        "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7",
        "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
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
fn draw_status(
    frame: &mut ratatui::Frame,
    hart: &Hart,
    _bus: &Bus,
    state: &TuiState,
    area: Rect,
) {
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
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
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

/// UART 输出面板
fn draw_uart(frame: &mut ratatui::Frame, state: &TuiState, area: Rect) {
    let block = Block::default()
        .title(" UART 输出 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    // 只显示最后能放下的行
    let inner_height = area.height.saturating_sub(2) as usize;
    let all_lines: Vec<&str> = state.uart_buffer.lines().collect();
    let start = all_lines.len().saturating_sub(inner_height);
    let visible: Vec<Line> = all_lines[start..]
        .iter()
        .map(|l| Line::from(l.to_string()))
        .collect();

    let paragraph = Paragraph::new(visible).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// 底部状态栏
fn draw_statusbar(frame: &mut ratatui::Frame, state: &TuiState, area: Rect) {
    let status = if state.finished {
        let reason = state
            .finish_reason
            .as_deref()
            .unwrap_or("done");
        format!(" ■ {} | q 退出", reason)
    } else if state.running {
        " ▶ 运行中 | Space 暂停 | q 退出".to_string()
    } else {
        " ⏸ 已暂停 | Space 继续 | q 退出".to_string()
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
