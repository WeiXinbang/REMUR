//! riscv-arch-test 框架回归测试
//!
//! 依赖：WSL + riscv64-unknown-elf-gcc + python3
//! 首次运行会自动 clone riscv-arch-test 并编译 ELF。

use std::process::Command;

#[test]
fn run_riscv_arch_framework() {
    let status = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "bash scripts/run-arch-test.sh",
            ])
            .status()
            .expect("failed to execute arch-test runner through bash")
    } else {
        Command::new("bash")
            .args(["scripts/run-arch-test.sh"])
            .status()
            .expect("failed to execute scripts/run-arch-test.sh")
    };

    assert!(status.success(), "riscv-arch-test framework failed");
}
