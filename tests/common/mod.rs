use remur::bus::{Bus, RAM_BASE};
use remur::cpu::Hart;
use remur::memory::Memory;

const TOHOST: u32 = 0x8000_1000;
const MEM_SIZE: usize = 128 * 1024 * 1024;
const MAX_CYCLES: u64 = 10_000_000;

pub fn run_test(bin_name: &str) {
    let path = format!("tests/bins/{}.bin", bin_name);
    let binary = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("Cannot read {}: {}", path, e));

    let mut mem = Memory::new(MEM_SIZE);
    mem.load_binary(0, &binary);

    let mut bus = Bus::new(mem);
    bus.tohost_addr = Some(TOHOST);

    let mut hart = Hart::new();
    hart.pc = RAM_BASE;

    match hart.run(&mut bus, MAX_CYCLES) {
        Some(1) => {} // PASS
        Some(val) => panic!("FAIL at test case {} (tohost=0x{:x})", val >> 1, val),
        None => panic!("Timeout after {} cycles, PC=0x{:08x}", MAX_CYCLES, hart.pc),
    }
}
