use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

mod bus;
#[cfg(feature = "cached-decode")]
mod cache;
mod clint;
mod cpu;
mod decode;
mod dtb;
mod execute;
mod instruction;
mod loader;
mod memory;
mod plic;
#[cfg(feature = "tui")]
mod tui;
mod uart;

const MEM_SIZE: usize = 128 * 1024 * 1024;
const DEFAULT_MAX_CYCLES: u64 = 10_000_000;
const DEFAULT_LINUX_MAX_CYCLES: u64 = 200_000_000;
const DEFAULT_KERNEL_ADDR: u32 = 0x8020_0000;
const DEFAULT_DTB_ADDR: u32 = 0x8200_0000;
const DEFAULT_INITRAMFS_ADDR: u32 = 0x8300_0000;
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
<<<<<<< Updated upstream
=======
    eprintln!("  --no-limit                   不限制执行周期数（运行直到程序结束）");
    eprintln!("  --tui                        TUI 仪表盘模式（需要 --features tui 编译）");
>>>>>>> Stashed changes
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

fn take_next(args: &[String], i: &mut usize, name: &str) -> String {
    *i += 1;
    if *i >= args.len() {
        panic!("Missing value for {}", name);
    }
    args[*i].clone()
}

fn parse_mode(args: &[String]) -> Mode {
    if args.len() < 2 {
        usage_and_exit();
    }

    let linux_mode =
        args.iter().any(|a| a == "--linux") || args.get(1).is_some_and(|a| a == "linux");

    if linux_mode {
        let mut kernel_file: Option<String> = None;
        let mut dtb_file: Option<String> = None;
        let mut initramfs_file: Option<String> = None;
        let mut kernel_addr = DEFAULT_KERNEL_ADDR;
        let mut dtb_addr = DEFAULT_DTB_ADDR;
        let mut initramfs_addr = DEFAULT_INITRAMFS_ADDR;
        let mut bootargs =
            "earlycon=uart8250,mmio,0x10000000 console=ttyS0 rdinit=/bin/sh".to_string();
        let mut max_cycles = DEFAULT_LINUX_MAX_CYCLES;
        let mut debug = DebugOptions::default();
        let mut use_tui = false;

        let mut i = 1usize;
        while i < args.len() {
            match args[i].as_str() {
                "linux" if i == 1 => {}
                "--linux" => {}
                "--tui" => use_tui = true,
                "--kernel" => kernel_file = Some(take_next(args, &mut i, "--kernel")),
                "--dtb" => dtb_file = Some(take_next(args, &mut i, "--dtb")),
                "--initramfs" => initramfs_file = Some(take_next(args, &mut i, "--initramfs")),
                "--kernel-addr" => {
                    let v = take_next(args, &mut i, "--kernel-addr");
                    kernel_addr = parse_u32_addr(&v, "--kernel-addr");
                }
                "--dtb-addr" => {
                    let v = take_next(args, &mut i, "--dtb-addr");
                    dtb_addr = parse_u32_addr(&v, "--dtb-addr");
                }
                "--initramfs-addr" => {
                    let v = take_next(args, &mut i, "--initramfs-addr");
                    initramfs_addr = parse_u32_addr(&v, "--initramfs-addr");
                }
                "--bootargs" => bootargs = take_next(args, &mut i, "--bootargs"),
                "--cycles" => {
                    let v = take_next(args, &mut i, "--cycles");
                    max_cycles = v
                        .parse::<u64>()
                        .unwrap_or_else(|_| panic!("Invalid --cycles: {}", v));
                }
                "--no-limit" => max_cycles = u64::MAX,
                "--itrace" => debug.itrace = true,
                "--itrace-file" => {
                    debug.itrace_file = Some(take_next(args, &mut i, "--itrace-file"))
                }
                "--itrace-limit" => {
                    let v = take_next(args, &mut i, "--itrace-limit");
                    debug.itrace_limit = Some(
                        v.parse::<u64>()
                            .unwrap_or_else(|_| panic!("Invalid --itrace-limit: {}", v)),
                    );
                }
                "--difftest-ref" => {
                    debug.difftest_ref = Some(take_next(args, &mut i, "--difftest-ref"))
                }
                "--difftest-ref-cmd" => {
                    debug.difftest_ref_cmd = Some(take_next(args, &mut i, "--difftest-ref-cmd"))
                }
                "--difftest-ref-out" => {
                    debug.difftest_ref_out = Some(take_next(args, &mut i, "--difftest-ref-out"))
                }
                other if other.starts_with("--") => {
                    panic!("Unknown option in linux mode: {}", other)
                }
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

        Mode::Linux(LinuxOptions {
            kernel_file,
            dtb_file,
            initramfs_file,
            kernel_addr,
            dtb_addr,
            initramfs_addr,
            bootargs,
            max_cycles,
            debug,
            tui: use_tui,
        })
    } else {
        let mut input_file: Option<String> = None;
        let mut tohost_override: Option<u32> = None;
        let mut signature_file: Option<String> = None;
        let mut max_cycles = DEFAULT_MAX_CYCLES;
        let mut debug = DebugOptions::default();
        let mut use_tui = false;

        let mut i = 1usize;
        while i < args.len() {
            match args[i].as_str() {
                "--tui" => use_tui = true,
                "--tohost" => {
                    let v = take_next(args, &mut i, "--tohost");
                    tohost_override = Some(parse_u32_addr(&v, "--tohost"));
                }
                "--signature" => {
                    signature_file = Some(take_next(args, &mut i, "--signature"));
                }
                "--cycles" => {
                    let v = take_next(args, &mut i, "--cycles");
                    max_cycles = v
                        .parse::<u64>()
                        .unwrap_or_else(|_| panic!("Invalid --cycles: {}", v));
                }
                "--no-limit" => max_cycles = u64::MAX,
                "--itrace" => debug.itrace = true,
                "--itrace-file" => {
                    debug.itrace_file = Some(take_next(args, &mut i, "--itrace-file"))
                }
                "--itrace-limit" => {
                    let v = take_next(args, &mut i, "--itrace-limit");
                    debug.itrace_limit = Some(
                        v.parse::<u64>()
                            .unwrap_or_else(|_| panic!("Invalid --itrace-limit: {}", v)),
                    );
                }
                "--difftest-ref" => {
                    debug.difftest_ref = Some(take_next(args, &mut i, "--difftest-ref"))
                }
                "--difftest-ref-cmd" => {
                    debug.difftest_ref_cmd = Some(take_next(args, &mut i, "--difftest-ref-cmd"))
                }
                "--difftest-ref-out" => {
                    debug.difftest_ref_out = Some(take_next(args, &mut i, "--difftest-ref-out"))
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

        let input_file = input_file.unwrap_or_else(|| usage_and_exit());
        Mode::Normal(NormalOptions {
            input_file,
            tohost_override,
            signature_file,
            max_cycles,
            debug,
            tui: use_tui,
        })
    }
}

struct DifftestContext<'a> {
    mode: &'a str,
    workload: &'a str,
    kernel: Option<&'a str>,
    bootargs: Option<&'a str>,
}

fn default_difftest_ref_out() -> String {
    PathBuf::from("target")
        .join("trace")
        .join("difftest.ref")
        .to_string_lossy()
        .into_owned()
}

fn run_difftest_ref_command(
    command: &str,
    out_path: &str,
    ctx: &DifftestContext<'_>,
) -> Result<(), String> {
    if let Some(parent) = Path::new(out_path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "cannot create difftest output directory '{}': {}",
                parent.display(),
                e
            )
        })?;
    }

    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            command,
        ]);
        c
    } else {
        let mut c = Command::new("bash");
        c.args(["-lc", command]);
        c
    };

    cmd.env("REMUR_DIFFTEST_REF_OUT", out_path)
        .env("REMUR_DIFFTEST_MODE", ctx.mode)
        .env("REMUR_DIFFTEST_WORKLOAD", ctx.workload);
    if let Some(kernel) = ctx.kernel {
        cmd.env("REMUR_DIFFTEST_KERNEL", kernel);
    }
    if let Some(bootargs) = ctx.bootargs {
        cmd.env("REMUR_DIFFTEST_BOOTARGS", bootargs);
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to start difftest-ref command: {}", e))?;
    if !status.success() {
        return Err(format!(
            "difftest-ref command failed with status {}",
            status
        ));
    }
    if !Path::new(out_path).is_file() {
        return Err(format!(
            "difftest-ref command succeeded but output file '{}' not found",
            out_path
        ));
    }
    Ok(())
}

fn prepare_difftest_ref(debug: &mut DebugOptions, ctx: &DifftestContext<'_>) -> Result<(), String> {
    if debug.difftest_ref_cmd.is_none() {
        if debug.difftest_ref_out.is_some() {
            return Err("--difftest-ref-out requires --difftest-ref-cmd".to_string());
        }
        return Ok(());
    }

    let out_path = debug
        .difftest_ref_out
        .clone()
        .or_else(|| debug.difftest_ref.clone())
        .unwrap_or_else(default_difftest_ref_out);

    let cmd = debug
        .difftest_ref_cmd
        .as_ref()
        .expect("checked above")
        .to_string();
    run_difftest_ref_command(&cmd, &out_path, ctx)?;
    debug.difftest_ref = Some(out_path);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// M7 difftest 事件流格式：
/// - I: 正常取指执行（含可选 trap 字段）
/// - T: 本步无取指、仅处理 trap/interrupt
enum DiffEvent {
    Inst {
        cycle: u64,
        pc: u32,
        inst: u32,
        next_pc: u32,
        priv_before: u8,
        priv_after: u8,
        trap_cause: Option<u32>,
        trap_tval: Option<u32>,
    },
    Trap {
        cycle: u64,
        pc: u32,
        next_pc: u32,
        priv_before: u8,
        priv_after: u8,
        cause: u32,
        tval: u32,
        interrupt: bool,
    },
}

fn priv_to_char(privilege: u8) -> char {
    match privilege {
        0 => 'U',
        1 => 'S',
        3 => 'M',
        _ => '?',
    }
}

fn parse_privilege(text: &str, field: &str) -> Result<u8, String> {
    match text {
        "U" => Ok(0),
        "S" => Ok(1),
        "M" => Ok(3),
        _ => Err(format!("invalid {} privilege '{}'", field, text)),
    }
}

fn parse_u32_token(token: &str, field: &str) -> Result<u32, String> {
    let t = token.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|_| format!("invalid {} '{}'", field, token))
    } else {
        t.parse::<u32>()
            .map_err(|_| format!("invalid {} '{}'", field, token))
    }
}

fn parse_u64_token(token: &str, field: &str) -> Result<u64, String> {
    token
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("invalid {} '{}'", field, token))
}

fn parse_optional_u32_token(token: &str, field: &str) -> Result<Option<u32>, String> {
    if token.trim() == "-" {
        Ok(None)
    } else {
        parse_u32_token(token, field).map(Some)
    }
}

fn format_diff_event(event: &DiffEvent) -> String {
    match event {
        DiffEvent::Inst {
            cycle,
            pc,
            inst,
            next_pc,
            priv_before,
            priv_after,
            trap_cause,
            trap_tval,
        } => format!(
            "I,{cycle},0x{pc:08x},0x{inst:08x},0x{next_pc:08x},{},{},{},{}",
            priv_to_char(*priv_before),
            priv_to_char(*priv_after),
            trap_cause
                .map(|v| format!("0x{v:08x}"))
                .unwrap_or_else(|| "-".to_string()),
            trap_tval
                .map(|v| format!("0x{v:08x}"))
                .unwrap_or_else(|| "-".to_string())
        ),
        DiffEvent::Trap {
            cycle,
            pc,
            next_pc,
            priv_before,
            priv_after,
            cause,
            tval,
            interrupt,
        } => format!(
            "T,{cycle},0x{pc:08x},0x{next_pc:08x},{},{},0x{cause:08x},0x{tval:08x},{}",
            priv_to_char(*priv_before),
            priv_to_char(*priv_after),
            if *interrupt { "int" } else { "exc" }
        ),
    }
}

fn parse_diff_event_line(line: &str, line_no: usize) -> Result<Option<DiffEvent>, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let parts: Vec<&str> = trimmed.split(',').collect();
    match parts.first().copied() {
        Some("I") => {
            if parts.len() != 9 {
                return Err(format!("line {}: invalid I record '{}'", line_no, trimmed));
            }
            Ok(Some(DiffEvent::Inst {
                cycle: parse_u64_token(parts[1], "cycle")?,
                pc: parse_u32_token(parts[2], "pc")?,
                inst: parse_u32_token(parts[3], "inst")?,
                next_pc: parse_u32_token(parts[4], "next_pc")?,
                priv_before: parse_privilege(parts[5], "priv_before")?,
                priv_after: parse_privilege(parts[6], "priv_after")?,
                trap_cause: parse_optional_u32_token(parts[7], "trap_cause")?,
                trap_tval: parse_optional_u32_token(parts[8], "trap_tval")?,
            }))
        }
        Some("T") => {
            if parts.len() != 9 {
                return Err(format!("line {}: invalid T record '{}'", line_no, trimmed));
            }
            let interrupt = match parts[8].trim() {
                "int" => true,
                "exc" => false,
                other => {
                    return Err(format!(
                        "line {}: invalid trap type '{}' (expected int/exc)",
                        line_no, other
                    ));
                }
            };
            Ok(Some(DiffEvent::Trap {
                cycle: parse_u64_token(parts[1], "cycle")?,
                pc: parse_u32_token(parts[2], "pc")?,
                next_pc: parse_u32_token(parts[3], "next_pc")?,
                priv_before: parse_privilege(parts[4], "priv_before")?,
                priv_after: parse_privilege(parts[5], "priv_after")?,
                cause: parse_u32_token(parts[6], "cause")?,
                tval: parse_u32_token(parts[7], "tval")?,
                interrupt,
            }))
        }
        Some(other) => Err(format!(
            "line {}: unknown record type '{}' in '{}'",
            line_no, other, trimmed
        )),
        None => Ok(None),
    }
}

fn diff_event_from_snapshot(cycle: u64, snap: &cpu::StepSnapshot) -> Option<DiffEvent> {
    if let Some(raw) = snap.raw_inst {
        let (trap_cause, trap_tval) = if let Some(trap) = snap.trap {
            (Some(trap.cause), Some(trap.tval))
        } else {
            (None, None)
        };
        return Some(DiffEvent::Inst {
            cycle,
            pc: snap.pc,
            inst: raw,
            next_pc: snap.next_pc,
            priv_before: snap.privilege_before,
            priv_after: snap.privilege_after,
            trap_cause,
            trap_tval,
        });
    }

    snap.trap.map(|trap| DiffEvent::Trap {
        cycle,
        pc: snap.pc,
        next_pc: snap.next_pc,
        priv_before: snap.privilege_before,
        priv_after: snap.privilege_after,
        cause: trap.cause,
        tval: trap.tval,
        interrupt: snap.interrupt_cause.is_some(),
    })
}

/// 管理 M7 运行期调试能力：
/// - itrace 输出（stderr 或文件）
/// - 与参考 trace 的逐步对拍
struct DebugRuntime {
    opts: DebugOptions,
    trace_output: Option<fs::File>,
    emitted: u64,
    diff_expected: Option<Vec<DiffEvent>>,
    diff_index: usize,
}

impl DebugRuntime {
    fn new(opts: DebugOptions) -> Result<Self, String> {
        let trace_output = match opts.itrace_file.as_ref() {
            Some(path) => Some(
                fs::File::create(path)
                    .map_err(|e| format!("cannot create itrace file '{}': {}", path, e))?,
            ),
            None => None,
        };

        let diff_expected = match opts.difftest_ref.as_ref() {
            Some(path) => {
                let file = fs::File::open(path)
                    .map_err(|e| format!("cannot open difftest ref '{}': {}", path, e))?;
                let reader = BufReader::new(file);
                let mut events = Vec::new();
                for (i, line) in reader.lines().enumerate() {
                    let line =
                        line.map_err(|e| format!("cannot read difftest ref '{}': {}", path, e))?;
                    if let Some(event) = parse_diff_event_line(&line, i + 1)? {
                        events.push(event);
                    }
                }
                Some(events)
            }
            None => None,
        };

        Ok(Self {
            opts,
            trace_output,
            emitted: 0,
            diff_expected,
            diff_index: 0,
        })
    }

    fn emit_trace_line(&mut self, line: &str) -> Result<(), String> {
        if let Some(limit) = self.opts.itrace_limit
            && self.emitted >= limit
        {
            return Ok(());
        }
        if self.opts.itrace {
            if let Some(file) = self.trace_output.as_mut() {
                writeln!(file, "{line}").map_err(|e| format!("write itrace failed: {}", e))?;
            } else {
                eprintln!("{line}");
            }
            self.emitted = self.emitted.saturating_add(1);
        }
        Ok(())
    }

    fn observe_step(&mut self, cycle: u64, snap: &cpu::StepSnapshot) -> Result<(), String> {
        let Some(actual) = diff_event_from_snapshot(cycle, snap) else {
            return Ok(());
        };

        self.emit_trace_line(&format_diff_event(&actual))?;

        if let Some(expected) = self.diff_expected.as_ref() {
            if self.diff_index >= expected.len() {
                return Err(format!(
                    "difftest mismatch at cycle {}: reference ended, actual={}",
                    cycle,
                    format_diff_event(&actual)
                ));
            }
            let want = &expected[self.diff_index];
            if &actual != want {
                return Err(format!(
                    "difftest mismatch at cycle {}:\n  expected: {}\n  actual:   {}",
                    cycle,
                    format_diff_event(want),
                    format_diff_event(&actual)
                ));
            }
            self.diff_index += 1;
        }
        Ok(())
    }

    fn finalize(&self) -> Result<(), String> {
        if let Some(expected) = self.diff_expected.as_ref()
            && self.diff_index != expected.len()
        {
            return Err(format!(
                "difftest mismatch: actual ended early, matched {} / {} events",
                self.diff_index,
                expected.len()
            ));
        }
        Ok(())
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
        let tui_cycles = if opts.max_cycles == DEFAULT_MAX_CYCLES {
            u64::MAX
        } else {
            opts.max_cycles
        };
        tui::run_tui_normal(&mut hart, &mut bus, tui_cycles);
        return;
    }
    #[cfg(not(feature = "tui"))]
    if opts.tui {
        panic!("TUI 功能未编译，请使用 --features tui 重新构建");
    }

    let mut debug_opts = opts.debug.clone();
    if let Err(e) = prepare_difftest_ref(
        &mut debug_opts,
        &DifftestContext {
            mode: "normal",
            workload: &opts.input_file,
            kernel: None,
            bootargs: None,
        },
    ) {
        panic!("{e}");
    }

    let mut debug_runtime = if debug_opts.needs_step_snapshots() {
        Some(DebugRuntime::new(debug_opts).unwrap_or_else(|e| panic!("{e}")))
    } else {
        None
    };

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

    if let Some(debug) = debug_runtime.as_ref()
        && let Err(e) = debug.finalize()
    {
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

fn extract_tar_gz(archive_bytes: &[u8], out_dir: &Path) {
    let gz = flate2::read::GzDecoder::new(archive_bytes);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(out_dir)
        .unwrap_or_else(|e| panic!("Failed to extract linux bundle: {}", e));
}

fn find_linux_bundle_url() -> Result<String, String> {
    let response =
        ureq::get("https://api.github.com/repos/sysprog21/rv32emu-prebuilt/releases?per_page=100")
            .set("User-Agent", "remur-linux-bootstrap")
            .call()
            .map_err(|e| format!("query releases failed: {}", e))?;

    let releases: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("invalid releases JSON: {}", e))?;

    let Some(release_array) = releases.as_array() else {
        return Err("unexpected releases JSON shape".to_string());
    };

    for release in release_array {
        let tag = release["tag_name"].as_str().unwrap_or_default();
        let name = release["name"].as_str().unwrap_or_default();
        if !(tag.contains("Linux-Image") || name.contains("Linux-Image")) {
            continue;
        }
        if let Some(assets) = release["assets"].as_array() {
            for asset in assets {
                let asset_name = asset["name"].as_str().unwrap_or_default();
                if asset_name == "rv32emu-linux-image-prebuilt.tar.gz" {
                    if let Some(url) = asset["browser_download_url"].as_str() {
                        return Ok(url.to_string());
                    }
                }
            }
        }
    }

    Err("no Linux-Image prebuilt asset found in releases".to_string())
}

#[cfg(windows)]
fn download_linux_bundle_via_powershell(out_dir: &Path) -> Result<Vec<u8>, String> {
    let archive_path = out_dir.join("rv32emu-linux-image-prebuilt.tar.gz");
    let archive_escaped = archive_path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop';\
         $ProgressPreference='SilentlyContinue';\
         $releases=Invoke-RestMethod -Headers @{{'User-Agent'='remur-linux-bootstrap'}} -Uri 'https://api.github.com/repos/sysprog21/rv32emu-prebuilt/releases?per_page=100';\
         $asset=$null;\
         foreach($r in $releases){{\
           if(($r.tag_name -like '*Linux-Image*') -or ($r.name -like '*Linux-Image*')){{\
             $asset=$r.assets | Where-Object {{ $_.name -eq 'rv32emu-linux-image-prebuilt.tar.gz' }} | Select-Object -First 1;\
             if($asset){{break}}\
           }}\
         }};\
         if(-not $asset){{throw 'Linux prebuilt asset not found';}};\
         Invoke-WebRequest -Uri $asset.browser_download_url -OutFile '{0}';",
        archive_escaped
    );

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|e| format!("failed to start powershell downloader: {}", e))?;

    if !status.success() {
        return Err("powershell downloader failed".to_string());
    }

    fs::read(&archive_path).map_err(|e| format!("failed to read downloaded archive: {}", e))
}

fn ensure_linux_prebuilt_bundle() -> (String, String) {
    let out_dir = PathBuf::from("target").join("linux-prebuilt");
    let linux_dir = out_dir
        .join("rv32emu-linux-image-prebuilt")
        .join("linux-image");
    let image = linux_dir.join("Image");
    let initrd = linux_dir.join("rootfs.cpio");

    if image.is_file() && initrd.is_file() {
        return (
            image.to_string_lossy().into_owned(),
            initrd.to_string_lossy().into_owned(),
        );
    }

    fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        panic!(
            "Failed to create linux cache dir '{}': {}",
            out_dir.display(),
            e
        )
    });

    let archive = match find_linux_bundle_url() {
        Ok(tar_url) => {
            eprintln!("Fetching Linux prebuilt bundle from {}", tar_url);
            match ureq::get(&tar_url)
                .set("User-Agent", "remur-linux-bootstrap")
                .call()
            {
                Ok(resp) => {
                    let mut archive = Vec::new();
                    let mut reader = resp.into_reader();
                    reader
                        .read_to_end(&mut archive)
                        .unwrap_or_else(|e| panic!("Failed to read linux prebuilt bundle: {}", e));
                    archive
                }
                Err(e) => {
                    #[cfg(windows)]
                    {
                        eprintln!(
                            "Direct TLS download failed ({}), fallback to PowerShell...",
                            e
                        );
                        download_linux_bundle_via_powershell(&out_dir).unwrap_or_else(|pe| {
                            panic!("Failed to download linux prebuilt bundle: {}", pe)
                        })
                    }
                    #[cfg(not(windows))]
                    {
                        panic!("Failed to download linux prebuilt bundle: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            #[cfg(windows)]
            {
                eprintln!(
                    "GitHub API via TLS failed ({}), fallback to PowerShell...",
                    e
                );
                download_linux_bundle_via_powershell(&out_dir)
                    .unwrap_or_else(|pe| panic!("Failed to download linux prebuilt bundle: {}", pe))
            }
            #[cfg(not(windows))]
            {
                panic!("Failed to resolve linux prebuilt bundle URL: {}", e);
            }
        }
    };

    extract_tar_gz(&archive, &out_dir);

    assert!(
        image.is_file() && initrd.is_file(),
        "Prebuilt linux bundle extracted but Image/rootfs.cpio missing under '{}'",
        linux_dir.display()
    );

    (
        image.to_string_lossy().into_owned(),
        initrd.to_string_lossy().into_owned(),
    )
}

fn ram_offset_for_load(addr: u32, low_ram_alias: bool) -> u32 {
    if low_ram_alias && addr < bus::RAM_BASE {
        addr
    } else {
        addr.wrapping_sub(bus::RAM_BASE)
    }
}

fn run_linux_mode(opts: LinuxOptions) {
    let LinuxOptions {
        kernel_file,
        dtb_file,
        initramfs_file,
        mut kernel_addr,
        mut dtb_addr,
        mut initramfs_addr,
        mut bootargs,
        max_cycles,
        mut debug,
        tui: use_tui,
    } = opts;

    let mem = memory::Memory::new(MEM_SIZE);
    let mut bus = bus::Bus::new(mem);
    let using_prebuilt_bundle = kernel_file.is_none();
    let use_rv32emu_profile = using_prebuilt_bundle
        && env::var("REMUR_LINUX_PROFILE")
            .map(|v| v.eq_ignore_ascii_case("rv32emu"))
            .unwrap_or(false);
    if using_prebuilt_bundle && !use_rv32emu_profile && kernel_addr == DEFAULT_KERNEL_ADDR {
        // Linux Image 头要求 kernel load address 4MiB 对齐，0x8020_0000 会在早期触发 ebreak。
        kernel_addr = 0x8040_0000;
    }
    if use_rv32emu_profile {
        bus.enable_rv32emu_linux_compat();
        if kernel_addr == DEFAULT_KERNEL_ADDR {
            kernel_addr = 0x0000_0000;
        }
        if dtb_addr == DEFAULT_DTB_ADDR {
            dtb_addr = 0x07F0_0000;
        }
        if initramfs_addr == DEFAULT_INITRAMFS_ADDR {
            initramfs_addr = 0x0700_0000;
        }
        if bootargs == "console=ttyS0 root=/dev/ram rw"
            || bootargs == "earlycon=uart8250,mmio,0x10000000 console=ttyS0"
            || bootargs == "earlycon=uart8250,mmio,0x10000000 console=ttyS0 rdinit=/bin/sh"
        {
            bootargs = "earlycon console=ttyS0".to_string();
        }
    }

    let (kernel_path, auto_initramfs) = match kernel_file {
        Some(path) => (path, None),
        None => {
            let (image, initrd) = ensure_linux_prebuilt_bundle();
            (image, Some(initrd))
        }
    };

    let kernel_data = fs::read(&kernel_path)
        .unwrap_or_else(|e| panic!("Failed to read kernel '{}': {}", kernel_path, e));
    let kernel_entry = if loader::is_elf(&kernel_data) {
        let info = loader::load_elf(&kernel_data, &mut bus);
        info.entry
    } else {
        let kernel_offset = ram_offset_for_load(kernel_addr, bus.is_low_ram_alias_enabled());
        bus.ram.load_binary(kernel_offset, &kernel_data);
        kernel_addr
    };

    let initramfs_file = initramfs_file.or(auto_initramfs);
    let initrd_bounds = if let Some(ref initramfs_file) = initramfs_file {
        let initrd = fs::read(initramfs_file)
            .unwrap_or_else(|e| panic!("Failed to read initramfs '{}': {}", initramfs_file, e));
        let offset = ram_offset_for_load(initramfs_addr, bus.is_low_ram_alias_enabled());
        bus.ram.load_binary(offset, &initrd);
        Some((
            initramfs_addr,
            initramfs_addr.wrapping_add(initrd.len() as u32),
        ))
    } else {
        None
    };

    let dtb_blob = if let Some(ref dtb_file) = dtb_file {
        fs::read(dtb_file).unwrap_or_else(|e| panic!("Failed to read DTB '{}': {}", dtb_file, e))
    } else if use_rv32emu_profile {
        dtb::build_rv32emu_compat_dtb(&bootargs, MEM_SIZE as u32, initrd_bounds)
    } else {
        dtb::build_default_dtb(&bootargs, bus::RAM_BASE, MEM_SIZE as u32, initrd_bounds)
    };
    let dtb_offset = ram_offset_for_load(dtb_addr, bus.is_low_ram_alias_enabled());
    bus.ram.load_binary(dtb_offset, &dtb_blob);

    let mut hart = cpu::Hart::new();
    hart.pc = kernel_entry;
    hart.privilege = 1; // Linux 内核运行在 S-mode
    // TUI 模式下不启用 stdin 桥接（由 TUI 自行处理键盘输入）
    if !use_tui {
        bus.uart.enable_host_input();
    }
    hart.enable_sbi(true);
    hart.write_reg(10, 0); // a0 = hartid
    hart.write_reg(11, dtb_addr); // a1 = FDT physical address
    hart.write_csr(cpu::MEDELEG, DELEGATE_EXCEPTIONS_TO_S);
    hart.write_csr(cpu::MIDELEG, cpu::MIP_SSIP | cpu::MIP_STIP | cpu::MIP_SEIP);
    hart.write_csr(cpu::MCOUNTEREN, COUNTER_EN_TM_IR);
    hart.write_csr(cpu::SCOUNTEREN, COUNTER_EN_TM_IR);

    println!(
        "Linux boot: entry=0x{:08x}, dtb=0x{:08x}, initramfs={}",
        kernel_entry,
        dtb_addr,
        initrd_bounds
            .map(|(s, _)| format!("0x{:08x}", s))
            .unwrap_or_else(|| "none".to_string())
    );

    #[cfg(feature = "tui")]
    if use_tui {
        bus.uart.enable_capture();
        // TUI 模式默认无限制（用户可手动退出）
        let tui_cycles = if max_cycles == DEFAULT_LINUX_MAX_CYCLES {
            u64::MAX
        } else {
            max_cycles
        };
        tui::run_tui_linux(&mut hart, &mut bus, tui_cycles);
        return;
    }
    #[cfg(not(feature = "tui"))]
    if use_tui {
        panic!("TUI 功能未编译，请使用 --features tui 重新构建");
    }

    if let Err(e) = prepare_difftest_ref(
        &mut debug,
        &DifftestContext {
            mode: "linux",
            workload: &kernel_path,
            kernel: Some(&kernel_path),
            bootargs: Some(&bootargs),
        },
    ) {
        panic!("{e}");
    }

    let mut debug_runtime = if debug.needs_step_snapshots() {
        Some(DebugRuntime::new(debug).unwrap_or_else(|e| panic!("{e}")))
    } else {
        None
    };

    for cycle in 0..max_cycles {
        if let Some(runtime) = debug_runtime.as_mut() {
            let snapshot = hart.step_snapshot(&mut bus);
            if let Err(e) = runtime.observe_step(cycle, &snapshot) {
                eprintln!("{e}");
                std::process::exit(3);
            }
        } else {
            hart.step(&mut bus);
        }
        if hart.shutdown_requested() {
            if let Some(runtime) = debug_runtime.as_ref()
                && let Err(e) = runtime.finalize()
            {
                eprintln!("{e}");
                std::process::exit(3);
            }
            println!("SBI shutdown");
            return;
        }
        if hart.pc == 0 {
            eprintln!("Fatal trap to 0 detected");
            break;
        }
    }

    if let Some(runtime) = debug_runtime.as_ref()
        && let Err(e) = runtime.finalize()
    {
        eprintln!("{e}");
        std::process::exit(3);
    }

    eprintln!("Timeout after {} cycles", max_cycles);
    eprintln!("PC  = 0x{:08x}", hart.pc);
    eprintln!("a0  = {} (0x{:08x})", hart.read_reg(10), hart.read_reg(10));
    eprintln!("priv= {}", hart.privilege);
    eprintln!(
        "mcause=0x{:08x} mepc=0x{:08x} mtval=0x{:08x}",
        hart.read_csr(cpu::MCAUSE),
        hart.read_csr(cpu::MEPC),
        hart.read_csr(cpu::MTVAL)
    );
    eprintln!(
        "scause=0x{:08x} sepc=0x{:08x} stval=0x{:08x}",
        hart.read_csr(cpu::SCAUSE),
        hart.read_csr(cpu::SEPC),
        hart.read_csr(cpu::STVAL)
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mode = parse_mode(&args);
    match mode {
        Mode::Normal(opts) => run_normal_mode(opts),
        Mode::Linux(opts) => run_linux_mode(opts),
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
