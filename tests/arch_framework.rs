//! riscv-arch-test 框架回归测试
//!
//! 依赖：WSL + riscv64-unknown-elf-gcc (>=15) + python3 + uv + sail_riscv_sim
//! 首次运行会自动安装 uv、clone riscv-arch-test 并编译 ELF。
//! 如果缺少依赖或版本不够，测试跳过而非失败。

use std::process::Command;

/// 检查 bash (WSL) 中是否存在某个命令
fn bash_has(cmd: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {} >/dev/null 2>&1", cmd)])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 获取 riscv64-unknown-elf-gcc 主版本号
fn gcc_major_version() -> Option<u32> {
    let output = Command::new("bash")
        .args(["-lc", "riscv64-unknown-elf-gcc -dumpversion"])
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout);
    version.trim().split('.').next()?.parse().ok()
}

#[test]
fn run_riscv_arch_framework() {
    // 前置检查：bash 可用
    if !bash_has("bash") {
        eprintln!("⏭ arch-test skipped: bash/WSL not available");
        return;
    }
    // 需要 sudo 安装的工具缺失时直接跳过
    if !bash_has("riscv64-unknown-elf-gcc") {
        eprintln!("⏭ arch-test skipped: riscv64-unknown-elf-gcc not found");
        eprintln!("  Install: sudo apt install gcc-riscv64-unknown-elf");
        return;
    }
    // ACT4 需要 GCC 15+
    match gcc_major_version() {
        Some(v) if v >= 15 => {}
        Some(v) => {
            eprintln!("⏭ arch-test skipped: GCC {v} found, ACT4 requires >= 15");
            eprintln!("  See: https://github.com/riscv/riscv-arch-test/tree/act4#3-risc-v-compiler");
            return;
        }
        None => {
            eprintln!("⏭ arch-test skipped: cannot determine GCC version");
            return;
        }
    }
    if !bash_has("python3") {
        eprintln!("⏭ arch-test skipped: python3 not found");
        return;
    }
    if !bash_has("sail_riscv_sim") {
        eprintln!("⏭ arch-test skipped: sail_riscv_sim not found");
        eprintln!("  Install: opam install sail && build sail-riscv from source");
        return;
    }

    // 在 Windows 侧编译 REMUR debug binary
    let build_status = Command::new("cargo")
        .args(["build", "--quiet"])
        .status()
        .expect("failed to run cargo build");
    assert!(build_status.success(), "cargo build failed");

    // 运行 arch-test 脚本（会自动安装 uv）
    let status = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "bash -l scripts/run-arch-test.sh --skip-build",
            ])
            .status()
            .expect("failed to execute arch-test runner through bash")
    } else {
        Command::new("bash")
            .args(["-l", "scripts/run-arch-test.sh", "--skip-build"])
            .status()
            .expect("failed to execute scripts/run-arch-test.sh")
    };

    assert!(status.success(), "riscv-arch-test framework failed");
}
