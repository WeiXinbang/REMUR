//! 二进制入口：
//! - 解析普通模式 / Linux 模式 CLI 参数
//! - 把 Linux 启动、difftest/itrace、TUI 分流到各自模块
//! - 保留 arch-test 所需的签名导出辅助逻辑

use std::env;
use std::fs;
use std::io::Write;

mod bus;
#[cfg(feature = "cached-decode")]
mod cache;
mod clint;
mod cpu;
pub(crate) mod debug_trace;
mod decode;
mod dtb;
mod execute;
mod instruction;
mod linux_boot;
mod loader;
mod memory;
mod plic;
#[cfg(feature = "tui")]
mod tui;
mod uart;

use crate::debug_trace::{DifftestContext, build_debug_runtime, ensure_debug_runtime_complete};

const MEM_SIZE: usize = 128 * 1024 * 1024;
const DEFAULT_MAX_CYCLES: u64 = 10_000_000;
const DEFAULT_LINUX_MAX_CYCLES: u64 = 200_000_000;
const DEFAULT_KERNEL_ADDR: u32 = 0x8020_0000;
const DEFAULT_DTB_ADDR: u32 = 0x8200_0000;
const DEFAULT_INITRAMFS_ADDR: u32 = 0x8300_0000;
const DEFAULT_LINUX_BOOTARGS: &str =
    "earlycon=uart8250,mmio,0x10000000 console=ttyS0 rdinit=/bin/sh";
const COUNTER_EN_TM_IR: u32 = (1 << 0) | (1 << 1) | (1 << 2);
const DELEGATE_EXCEPTIONS_TO_S: u32 = (1 << 0) |  // instruction address misaligned
    (1 << 1) |  // instruction access fault
    (1 << 2) |  // illegal instruction
    (1 << 3) |  // breakpoint
    (1 << 4) |  // load address misaligned
    (1 << 5) |  // load access fault
    (1 << 6) |  // store/AMO address misaligned
    (1 << 7) |  // store/AMO access fault
    (1 << 8) |  // ecall from U
    (1 << 9) |  // ecall from S
    (1 << 12) | // instruction page fault
    (1 << 13) | // load page fault
    (1 << 15); // store/AMO page fault

struct NormalOptions {
    input_file: String,
    tohost_override: Option<u32>,
    signature_file: Option<String>,
    max_cycles: u64,
    debug: DebugOptions,
    tui: bool,
}

struct LinuxOptions {
    kernel_file: Option<String>,
    dtb_file: Option<String>,
    initramfs_file: Option<String>,
    kernel_addr: u32,
    dtb_addr: u32,
    initramfs_addr: u32,
    bootargs: String,
    max_cycles: u64,
    debug: DebugOptions,
    tui: bool,
}

#[derive(Default, Clone)]
struct DebugOptions {
    itrace: bool,
    itrace_file: Option<String>,
    itrace_limit: Option<u64>,
    difftest_ref: Option<String>,
    difftest_ref_cmd: Option<String>,
    difftest_ref_out: Option<String>,
}

impl DebugOptions {
    fn needs_step_snapshots(&self) -> bool {
        self.itrace || self.difftest_ref.is_some() || self.difftest_ref_cmd.is_some()
    }
}

enum Mode {
    Normal(NormalOptions),
    Linux(LinuxOptions),
}

fn usage_and_exit() -> ! {
    eprintln!("Usage:");
    eprintln!("  remur <binary_or_elf> [options]");
    eprintln!("  remur linux --kernel <image_or_elf> [linux-options]");
    eprintln!("  remur --linux --kernel <image_or_elf> [linux-options]");
    eprintln!();
    eprintln!("Common options:");
    eprintln!(
        "  --cycles <n>                 最大执行周期数（普通模式默认 10M，Linux 模式默认 200M）"
    );
    eprintln!("  --no-limit                   不限制执行周期数（运行直到程序结束）");
    eprintln!("  --tui                        TUI 仪表盘模式（需要 --features tui 编译）");
    eprintln!("  --itrace                     输出指令级 trace（M7）");
    eprintln!("  --itrace-file <path>         将 trace 写入文件（默认 stderr）");
    eprintln!("  --itrace-limit <n>           最多输出 n 条 trace");
    eprintln!("  --difftest-ref <path>        与参考 trace 逐步对比（M7 最小 difftest）");
    eprintln!("  --difftest-ref-cmd <cmd>     先运行外部命令生成参考 trace（M7.2）");
    eprintln!(
        "  --difftest-ref-out <path>    外部命令输出参考 trace 路径（默认 target\\trace\\difftest.ref）"
    );
    eprintln!();
    eprintln!("Normal mode options:");
    eprintln!("  --tohost <hex_addr>          tohost 地址（raw .bin 用），ELF 自动解析");
    eprintln!("  --signature <output_file>    运行后导出签名区域（arch-test 用）");
    eprintln!();
    eprintln!("Linux mode options (M6):");
    eprintln!("  --kernel <file>              Linux Image/raw/ELF（可省略：自动下载预构建 Linux）");
    eprintln!("  --dtb <file>                 外部 DTB 文件（不传则内置生成）");
    eprintln!("  --initramfs <file>           initramfs 文件（可选）");
    eprintln!(
        "  --kernel-addr <hex_addr>     raw Image 加载地址（默认 0x80200000；预构建镜像自动对齐到 0x80400000）"
    );
    eprintln!("  --dtb-addr <hex_addr>        DTB 加载地址（默认 0x82000000）");
    eprintln!("  --initramfs-addr <hex_addr>  initramfs 加载地址（默认 0x83000000）");
    eprintln!("  --bootargs <string>          chosen/bootargs");
    std::process::exit(1);
}

fn parse_u32_addr(text: &str, name: &str) -> u32 {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16)
            .unwrap_or_else(|_| panic!("Invalid {}: {}", name, text));
    }
    if t.chars().all(|c| c.is_ascii_hexdigit()) {
        return u32::from_str_radix(t, 16).unwrap_or_else(|_| panic!("Invalid {}: {}", name, text));
    }
    t.parse::<u32>()
        .unwrap_or_else(|_| panic!("Invalid {}: {}", name, text))
}

fn take_required_value(args: &[String], i: &mut usize, name: &str) -> String {
    *i += 1;
    if *i >= args.len() {
        panic!("Missing value for {}", name);
    }
    args[*i].clone()
}

fn parse_u64_arg(args: &[String], i: &mut usize, name: &str) -> u64 {
    let value = take_required_value(args, i, name);
    value
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("Invalid {}: {}", name, value))
}

/// 解析普通模式 / Linux 模式共享的运行时开关。
fn parse_shared_runtime_option(
    arg: &str,
    args: &[String],
    i: &mut usize,
    max_cycles: &mut u64,
    debug: &mut DebugOptions,
    enable_tui: &mut bool,
) -> bool {
    match arg {
        "--tui" => *enable_tui = true,
        "--cycles" => *max_cycles = parse_u64_arg(args, i, "--cycles"),
        "--no-limit" => *max_cycles = u64::MAX,
        "--itrace" => debug.itrace = true,
        "--itrace-file" => debug.itrace_file = Some(take_required_value(args, i, "--itrace-file")),
        "--itrace-limit" => debug.itrace_limit = Some(parse_u64_arg(args, i, "--itrace-limit")),
        "--difftest-ref" => {
            debug.difftest_ref = Some(take_required_value(args, i, "--difftest-ref"))
        }
        "--difftest-ref-cmd" => {
            debug.difftest_ref_cmd = Some(take_required_value(args, i, "--difftest-ref-cmd"))
        }
        "--difftest-ref-out" => {
            debug.difftest_ref_out = Some(take_required_value(args, i, "--difftest-ref-out"))
        }
        _ => return false,
    }
    true
}

fn is_linux_mode(args: &[String]) -> bool {
    args.iter().any(|a| a == "--linux") || args.get(1).is_some_and(|a| a == "linux")
}

/// Linux 模式在独有选项之上，复用共享运行时开关解析。
fn parse_linux_options(args: &[String]) -> LinuxOptions {
    let mut kernel_file: Option<String> = None;
    let mut dtb_file: Option<String> = None;
    let mut initramfs_file: Option<String> = None;
    let mut kernel_addr = DEFAULT_KERNEL_ADDR;
    let mut dtb_addr = DEFAULT_DTB_ADDR;
    let mut initramfs_addr = DEFAULT_INITRAMFS_ADDR;
    let mut bootargs = DEFAULT_LINUX_BOOTARGS.to_string();
    let mut max_cycles = DEFAULT_LINUX_MAX_CYCLES;
    let mut debug = DebugOptions::default();
    let mut enable_tui = false;

    let mut i = 1usize;
    while i < args.len() {
        let arg = args[i].as_str();
        if parse_shared_runtime_option(
            arg,
            args,
            &mut i,
            &mut max_cycles,
            &mut debug,
            &mut enable_tui,
        ) {
            i += 1;
            continue;
        }

        match arg {
            "linux" if i == 1 => {}
            "--linux" => {}
            "--kernel" => kernel_file = Some(take_required_value(args, &mut i, "--kernel")),
            "--dtb" => dtb_file = Some(take_required_value(args, &mut i, "--dtb")),
            "--initramfs" => {
                initramfs_file = Some(take_required_value(args, &mut i, "--initramfs"))
            }
            "--kernel-addr" => {
                let value = take_required_value(args, &mut i, "--kernel-addr");
                kernel_addr = parse_u32_addr(&value, "--kernel-addr");
            }
            "--dtb-addr" => {
                let value = take_required_value(args, &mut i, "--dtb-addr");
                dtb_addr = parse_u32_addr(&value, "--dtb-addr");
            }
            "--initramfs-addr" => {
                let value = take_required_value(args, &mut i, "--initramfs-addr");
                initramfs_addr = parse_u32_addr(&value, "--initramfs-addr");
            }
            "--bootargs" => bootargs = take_required_value(args, &mut i, "--bootargs"),
            other if other.starts_with("--") => panic!("Unknown option in linux mode: {}", other),
            other => {
                if kernel_file.is_none() {
                    kernel_file = Some(other.to_string());
                } else {
                    panic!("Unexpected positional argument: {}", other);
                }
            }
        }
        i += 1;
    }

    LinuxOptions {
        kernel_file,
        dtb_file,
        initramfs_file,
        kernel_addr,
        dtb_addr,
        initramfs_addr,
        bootargs,
        max_cycles,
        debug,
        tui: enable_tui,
    }
}

/// 普通模式和 Linux 模式共享调试/TUI 相关开关，只保留 workload 特有部分。
fn parse_normal_options(args: &[String]) -> NormalOptions {
    let mut input_file: Option<String> = None;
    let mut tohost_override: Option<u32> = None;
    let mut signature_file: Option<String> = None;
    let mut max_cycles = DEFAULT_MAX_CYCLES;
    let mut debug = DebugOptions::default();
    let mut enable_tui = false;

    let mut i = 1usize;
    while i < args.len() {
        let arg = args[i].as_str();
        if parse_shared_runtime_option(
            arg,
            args,
            &mut i,
            &mut max_cycles,
            &mut debug,
            &mut enable_tui,
        ) {
            i += 1;
            continue;
        }

        match arg {
            "--tohost" => {
                let value = take_required_value(args, &mut i, "--tohost");
                tohost_override = Some(parse_u32_addr(&value, "--tohost"));
            }
            "--signature" => {
                signature_file = Some(take_required_value(args, &mut i, "--signature"))
            }
            "--linux" => panic!("Use --linux with --kernel for Linux boot mode"),
            other if other.starts_with("--") => panic!("Unknown option: {}", other),
            other => {
                if input_file.is_none() {
                    input_file = Some(other.to_string());
                } else if tohost_override.is_none() {
                    // 向后兼容：第二个位置参数作为 tohost 地址
                    tohost_override = Some(parse_u32_addr(other, "tohost"));
                } else {
                    panic!("Unexpected positional argument: {}", other);
                }
            }
        }
        i += 1;
    }

    NormalOptions {
        input_file: input_file.unwrap_or_else(|| usage_and_exit()),
        tohost_override,
        signature_file,
        max_cycles,
        debug,
        tui: enable_tui,
    }
}

fn parse_mode(args: &[String]) -> Mode {
    if args.len() < 2 {
        usage_and_exit();
    }

    if is_linux_mode(args) {
        Mode::Linux(parse_linux_options(args))
    } else {
        Mode::Normal(parse_normal_options(args))
    }
}

/// TUI 默认应持续运行直到用户主动退出；只有显式传入 --cycles 时才保留上限。
#[cfg(feature = "tui")]
fn resolve_tui_cycles(max_cycles: u64, default_cycles: u64) -> u64 {
    if max_cycles == default_cycles {
        u64::MAX
    } else {
        max_cycles
    }
}

fn run_normal_mode(opts: NormalOptions) {
    let data = fs::read(&opts.input_file).expect("Failed to read input file");
    let mem = memory::Memory::new(MEM_SIZE);
    let mut bus = bus::Bus::new(mem);

    let (entry, tohost, sig_bounds) = if loader::is_elf(&data) {
        let info = loader::load_elf(&data, &mut bus);
        let tohost = opts.tohost_override.or(info.tohost);
        let sig = match (info.begin_signature, info.end_signature) {
            (Some(begin), Some(end)) => Some((begin, end)),
            _ => None,
        };
        (info.entry, tohost, sig)
    } else {
        bus.ram.load_binary(0, &data);
        (bus::RAM_BASE, opts.tohost_override, None)
    };

    bus.tohost_addr = tohost;
    let mut hart = cpu::Hart::new();
    hart.pc = entry;

    #[cfg(feature = "tui")]
    if opts.tui {
        bus.uart.enable_capture();
        // TUI 模式默认无限制（用户可手动退出）
        let tui_cycles = resolve_tui_cycles(opts.max_cycles, DEFAULT_MAX_CYCLES);
        tui::run_tui_normal(&mut hart, &mut bus, tui_cycles);
        return;
    }
    #[cfg(not(feature = "tui"))]
    if opts.tui {
        panic!("TUI 功能未编译，请使用 --features tui 重新构建");
    }

    let mut debug_runtime = build_debug_runtime(
        opts.debug.clone(),
        DifftestContext {
            mode: "normal",
            workload: &opts.input_file,
            kernel: None,
            bootargs: None,
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));

    let mut tohost_result = None;
    for cycle in 0..opts.max_cycles {
        if let Some(debug) = debug_runtime.as_mut() {
            let snapshot = hart.step_snapshot(&mut bus);
            if let Err(e) = debug.observe_step(cycle, &snapshot) {
                eprintln!("{e}");
                std::process::exit(3);
            }
        } else {
            hart.step(&mut bus);
        }

        if let Some(val) = bus.tohost_value {
            tohost_result = Some(val);
            break;
        }
    }

    if let Err(e) = ensure_debug_runtime_complete(debug_runtime.as_ref()) {
        eprintln!("{e}");
        std::process::exit(3);
    }

    match tohost_result {
        Some(val) if val == 1 => println!("PASS"),
        Some(val) => {
            let test_num = val >> 1;
            println!("FAIL at test case {}", test_num);
            std::process::exit(1);
        }
        None => {
            eprintln!("Timeout after {} cycles", opts.max_cycles);
            eprintln!("PC  = 0x{:08x}", hart.pc);
            eprintln!("a0  = {} (0x{:08x})", hart.read_reg(10), hart.read_reg(10));
            if let Some(ref path) = opts.signature_file
                && let Some((begin, end)) = sig_bounds
            {
                dump_signature(&bus, begin, end, path);
            }
            std::process::exit(2);
        }
    }

    if let Some(ref path) = opts.signature_file {
        if let Some((begin, end)) = sig_bounds {
            dump_signature(&bus, begin, end, path);
        } else {
            eprintln!(
                "Warning: --signature specified but ELF has no begin_signature/end_signature symbols"
            );
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mode = parse_mode(&args);
    match mode {
        Mode::Normal(opts) => run_normal_mode(opts),
        Mode::Linux(opts) => linux_boot::run_linux_mode(opts),
    }
}

/// 将签名区域按 arch-test 格式导出：每行一个 32-bit word（8 位小写 hex）
fn dump_signature(bus: &bus::Bus, begin: u32, end: u32, path: &str) {
    let mut file = fs::File::create(path)
        .unwrap_or_else(|e| panic!("Cannot create signature file '{}': {}", path, e));

    let mut addr = begin;
    while addr < end {
        let offset = addr.wrapping_sub(bus::RAM_BASE);
        let word = bus.ram.read32(offset);
        writeln!(file, "{:08x}", word).unwrap_or_else(|e| panic!("Write signature failed: {}", e));
        addr += 4;
    }
    eprintln!(
        "Signature dumped: 0x{:08x}..0x{:08x} -> {}",
        begin, end, path
    );
}
