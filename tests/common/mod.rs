use remur::bus::{Bus, RAM_BASE};
use remur::cpu::Hart;
use remur::loader;
use remur::memory::Memory;
use std::sync::OnceLock;

const TOHOST: u32 = 0x8000_1000;
const MEM_SIZE: usize = 128 * 1024 * 1024;
const MAX_CYCLES: u64 = 10_000_000;
static ENSURE_TEST_BINS: OnceLock<()> = OnceLock::new();

fn ensure_test_binaries() {
    ENSURE_TEST_BINS.get_or_init(|| {
        let status = if cfg!(windows) {
            std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    "scripts\\setup_tests.ps1",
                ])
                .status()
                .expect("failed to execute scripts\\setup_tests.ps1")
        } else {
            std::process::Command::new("bash")
                .args(["scripts/compile_tests.sh"])
                .status()
                .expect("failed to execute scripts/compile_tests.sh")
        };
        assert!(status.success(), "failed to build riscv-tests binaries");
    });
}

pub fn run_test(bin_name: &str) {
    // 优先尝试 ELF，不存在则回退到 raw .bin
    let elf_path = format!("tests/bins/{}.elf", bin_name);
    let bin_path = format!("tests/bins/{}.bin", bin_name);
    if !std::path::Path::new(&elf_path).exists() && !std::path::Path::new(&bin_path).exists() {
        ensure_test_binaries();
    }

    let mem = Memory::new(MEM_SIZE);
    let mut bus = Bus::new(mem);
    let mut hart = Hart::new();

    if let Ok(data) = std::fs::read(&elf_path) {
        let info = loader::load_elf(&data, &mut bus);
        bus.tohost_addr = info.tohost.or(Some(TOHOST));
        hart.pc = info.entry;
    } else {
        let binary = std::fs::read(&bin_path)
            .unwrap_or_else(|e| panic!("Cannot read {} or {}: {}", elf_path, bin_path, e));
        bus.ram.load_binary(0, &binary);
        bus.tohost_addr = Some(TOHOST);
        hart.pc = RAM_BASE;
    }

    match hart.run(&mut bus, MAX_CYCLES) {
        Some(1) => {} // PASS
        Some(val) => panic!("FAIL at test case {} (tohost=0x{:x})", val >> 1, val),
        None => panic!("Timeout after {} cycles, PC=0x{:08x}", MAX_CYCLES, hart.pc),
    }
}
