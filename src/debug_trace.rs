//! M7 调试/对拍辅助：
//! - difftest 参考 trace 的生成入口
//! - trace/difftest 事件格式的解析与序列化
//! - 运行时的 itrace 输出与逐步对拍

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{DebugOptions, cpu};

#[derive(Clone, Copy, Debug)]
pub(crate) struct DifftestContext<'a> {
    pub mode: &'a str,
    pub workload: &'a str,
    pub kernel: Option<&'a str>,
    pub bootargs: Option<&'a str>,
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

pub(crate) fn prepare_difftest_ref(
    debug: &mut DebugOptions,
    ctx: &DifftestContext<'_>,
) -> Result<(), String> {
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
pub(crate) struct DebugRuntime {
    opts: DebugOptions,
    trace_output: Option<fs::File>,
    emitted: u64,
    diff_expected: Option<Vec<DiffEvent>>,
    diff_index: usize,
}

impl DebugRuntime {
    pub(crate) fn new(opts: DebugOptions) -> Result<Self, String> {
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

    pub(crate) fn observe_step(
        &mut self,
        cycle: u64,
        snap: &cpu::StepSnapshot,
    ) -> Result<(), String> {
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

    pub(crate) fn finalize(&self) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{StepSnapshot, TrapSnapshot};

    #[test]
    fn parse_and_format_inst_event_round_trip() {
        let line = "I,7,0x80000000,0x00000073,0x80000004,S,S,-,-";
        let event = parse_diff_event_line(line, 1).unwrap().unwrap();
        assert_eq!(format_diff_event(&event), line);
    }

    #[test]
    fn diff_event_from_snapshot_emits_trap_record() {
        let snapshot = StepSnapshot {
            pc: 0x8000_0000,
            privilege_before: 1,
            phys_pc: None,
            raw_inst: None,
            decoded_inst: None,
            next_pc: 0x8000_0100,
            privilege_after: 3,
            interrupt_cause: Some(0x8000_0007),
            trap: Some(TrapSnapshot {
                cause: 0x8000_0007,
                tval: 0,
                from_privilege: 1,
                to_privilege: 3,
                delegated_to_s: false,
                trap_vector: 0x8000_0100,
            }),
        };

        let event = diff_event_from_snapshot(42, &snapshot).unwrap();
        assert_eq!(
            format_diff_event(&event),
            "T,42,0x80000000,0x80000100,S,M,0x80000007,0x00000000,int"
        );
    }

    #[test]
    fn prepare_difftest_ref_rejects_out_without_cmd() {
        let mut debug = DebugOptions {
            difftest_ref_out: Some("target\\trace\\manual.ref".to_string()),
            ..DebugOptions::default()
        };

        let err = prepare_difftest_ref(
            &mut debug,
            &DifftestContext {
                mode: "normal",
                workload: "demo.elf",
                kernel: None,
                bootargs: None,
            },
        )
        .unwrap_err();

        assert_eq!(err, "--difftest-ref-out requires --difftest-ref-cmd");
    }
}
