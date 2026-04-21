//! riscv-arch-test 框架回归测试
//!
//! 依赖：WSL(Windows) / bash(Linux) + riscv64-unknown-elf-gcc (>=15) + python3 + uv
//! 首次运行会自动安装 uv、clone riscv-arch-test 并编译 ELF。
//! 如果缺少依赖或版本不够，测试跳过而非失败。

use std::process::Command;

#[cfg(windows)]
fn arch_test_shell(command: &str) -> Command {
    let mut cmd = Command::new("wsl");
    cmd.args(["bash", "-lc", command]);
    cmd
}

#[cfg(not(windows))]
fn arch_test_shell(command: &str) -> Command {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", command]);
    cmd
}

fn arch_test_has(cmd: &str) -> bool {
    arch_test_shell(&format!("command -v {} >/dev/null 2>&1", cmd))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn gcc_major_version() -> Option<u32> {
    let output = arch_test_shell("riscv64-unknown-elf-gcc -dumpversion")
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout);
    version.trim().split('.').next()?.parse().ok()
}

#[cfg(windows)]
fn manifest_dir_for_arch_test() -> Option<String> {
    let output = Command::new("wsl")
        .args(["wslpath", "-a", env!("CARGO_MANIFEST_DIR")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn sh_single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

#[test]
fn run_riscv_arch_framework() {
    if !arch_test_has("bash") {
        eprintln!("⏭ arch-test skipped: bash/WSL not available");
        return;
    }
    if !arch_test_has("riscv64-unknown-elf-gcc") {
        eprintln!("⏭ arch-test skipped: riscv64-unknown-elf-gcc not found");
        eprintln!("  Install: sudo apt install gcc-riscv64-unknown-elf");
        return;
    }
    // ACT4 需要 GCC 15+
    match gcc_major_version() {
        Some(v) if v >= 15 => {}
        Some(v) => {
            eprintln!("⏭ arch-test skipped: GCC {v} found, ACT4 requires >= 15");
            eprintln!(
                "  See: https://github.com/riscv/riscv-arch-test/tree/act4#3-risc-v-compiler"
            );
            return;
        }
        None => {
            eprintln!("⏭ arch-test skipped: cannot determine GCC version");
            return;
        }
    }
    if !arch_test_has("python3") {
        eprintln!("⏭ arch-test skipped: python3 not found");
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
        let Some(manifest_dir) = manifest_dir_for_arch_test() else {
            eprintln!("⏭ arch-test skipped: cannot convert project directory to a WSL path");
            return;
        };
        arch_test_shell(&format!(
            "cd {} && scripts/run-arch-test.sh --skip-build",
            sh_single_quote(&manifest_dir)
        ))
        .status()
        .expect("failed to execute arch-test runner through WSL")
    } else {
        arch_test_shell("scripts/run-arch-test.sh --skip-build")
            .status()
            .expect("failed to execute scripts/run-arch-test.sh")
    };

    assert!(status.success(), "riscv-arch-test framework failed");
}
