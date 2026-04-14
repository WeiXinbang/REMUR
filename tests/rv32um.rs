use REMUR::cpu::Hart;
use REMUR::memory::Memory;

const BASE: u32 = 0x8000_0000;
const TOHOST: u32 = 0x8000_1000;
const MEM_SIZE: usize = 128 * 1024 * 1024;
const MAX_CYCLES: u64 = 10_000_000;

fn run_test(bin_name: &str) {
    let path = format!("tests/bins/{}.bin", bin_name);
    let binary = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("Cannot read {}: {}", path, e));

    let mut mem = Memory::new(MEM_SIZE, BASE);
    mem.load_binary(BASE, &binary);

    let mut hart = Hart::new();
    hart.pc = BASE;
    hart.tohost_addr = Some(TOHOST);

    match hart.run(&mut mem, MAX_CYCLES) {
        Some(1) => {}
        Some(val) => panic!("FAIL at test case {} (tohost=0x{:x})", val >> 1, val),
        None => panic!("Timeout after {} cycles, PC=0x{:08x}", MAX_CYCLES, hart.pc),
    }
}

macro_rules! riscv_tests {
    ($prefix:expr, $($name:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                run_test(&format!("{}-{}", $prefix, stringify!($name)));
            }
        )+
    };
}

riscv_tests! { "rv32um-p",
    mul, mulh, mulhsu, mulhu,
    div, divu, rem, remu,
}
