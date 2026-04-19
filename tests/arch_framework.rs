//! riscv-arch-test 框架回归测试（默认忽略）
//!
//! 运行方式：
//!   cargo arch-test
//! 或
//!   cargo test --test arch_framework -- --ignored --nocapture

use std::process::Command;

#[test]
#[ignore = "requires Linux/WSL toolchain and may take a long time"]
fn run_riscv_arch_framework() {
    let status = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "bash scripts/run-arch-test.sh --skip-build",
            ])
            .status()
            .expect("failed to execute arch-test runner through bash")
    } else {
        Command::new("bash")
            .args(["scripts/run-arch-test.sh", "--skip-build"])
            .status()
            .expect("failed to execute scripts/run-arch-test.sh")
    };

    assert!(status.success(), "riscv-arch-test framework failed");
}
