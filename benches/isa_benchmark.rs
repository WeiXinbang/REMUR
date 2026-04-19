use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use remur::bus::{Bus, RAM_BASE};
use remur::cpu::Hart;
use remur::memory::Memory;

const TOHOST: u32 = 0x8000_1000;
const MEM_SIZE: usize = 128 * 1024 * 1024;
const MAX_CYCLES: u64 = 10_000_000;

fn run_test_return_cycles(bin_name: &str) -> u64 {
    let path = format!("tests/bins/{}.bin", bin_name);
    let binary = std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read {}: {}", path, e));

    let mut mem = Memory::new(MEM_SIZE);
    mem.load_binary(0, &binary);

    let mut bus = Bus::new(mem);
    bus.tohost_addr = Some(TOHOST);

    let mut hart = Hart::new();
    hart.pc = RAM_BASE;

    let mut cycles: u64 = 0;
    for i in 0..MAX_CYCLES {
        hart.step(&mut bus);
        if bus.tohost_value.is_some() {
            cycles = i + 1;
            break;
        }
    }
    assert_eq!(
        bus.tohost_value,
        Some(1),
        "{} FAIL: tohost = {:?}",
        bin_name,
        bus.tohost_value
    );
    cycles
}

fn bench_single_test(c: &mut Criterion, name: &str, bin_name: &str) {
    c.bench_with_input(
        BenchmarkId::new("riscv-test", name),
        &bin_name,
        |b, &bin| {
            b.iter(|| run_test_return_cycles(bin));
        },
    );
}

fn bench_rv32ui(c: &mut Criterion) {
    let tests = [
        ("add", "rv32ui-p-add"),
        ("jal", "rv32ui-p-jal"),
        ("lw", "rv32ui-p-lw"),
        ("sw", "rv32ui-p-sw"),
        ("beq", "rv32ui-p-beq"),
    ];
    for (name, bin) in &tests {
        bench_single_test(c, name, bin);
    }
}

fn bench_rv32um(c: &mut Criterion) {
    let tests = [("mul", "rv32um-p-mul"), ("div", "rv32um-p-div")];
    for (name, bin) in &tests {
        bench_single_test(c, name, bin);
    }
}

fn bench_rv32ua(c: &mut Criterion) {
    bench_single_test(c, "amoadd_w", "rv32ua-p-amoadd_w");
}

fn bench_rv32mi(c: &mut Criterion) {
    let tests = [("csr", "rv32mi-p-csr"), ("illegal", "rv32mi-p-illegal")];
    for (name, bin) in &tests {
        bench_single_test(c, name, bin);
    }
}

fn bench_rv32si(c: &mut Criterion) {
    bench_single_test(c, "dirty", "rv32si-p-dirty");
}

/// Aggregate benchmark: run ALL 77 tests as one workload
fn bench_all_tests(c: &mut Criterion) {
    let all_bins: Vec<&str> = vec![
        // rv32ui (37)
        "rv32ui-p-add",
        "rv32ui-p-addi",
        "rv32ui-p-and",
        "rv32ui-p-andi",
        "rv32ui-p-auipc",
        "rv32ui-p-beq",
        "rv32ui-p-bge",
        "rv32ui-p-bgeu",
        "rv32ui-p-blt",
        "rv32ui-p-bltu",
        "rv32ui-p-bne",
        "rv32ui-p-jal",
        "rv32ui-p-jalr",
        "rv32ui-p-lb",
        "rv32ui-p-lbu",
        "rv32ui-p-lh",
        "rv32ui-p-lhu",
        "rv32ui-p-lui",
        "rv32ui-p-lw",
        "rv32ui-p-or",
        "rv32ui-p-ori",
        "rv32ui-p-sb",
        "rv32ui-p-sh",
        "rv32ui-p-sll",
        "rv32ui-p-slli",
        "rv32ui-p-slt",
        "rv32ui-p-slti",
        "rv32ui-p-sltiu",
        "rv32ui-p-sltu",
        "rv32ui-p-sra",
        "rv32ui-p-srai",
        "rv32ui-p-srl",
        "rv32ui-p-srli",
        "rv32ui-p-sub",
        "rv32ui-p-sw",
        "rv32ui-p-xor",
        "rv32ui-p-xori",
        // rv32um (8)
        "rv32um-p-mul",
        "rv32um-p-mulh",
        "rv32um-p-mulhsu",
        "rv32um-p-mulhu",
        "rv32um-p-div",
        "rv32um-p-divu",
        "rv32um-p-rem",
        "rv32um-p-remu",
        // rv32ua (10)
        "rv32ua-p-amoadd_w",
        "rv32ua-p-amoand_w",
        "rv32ua-p-amomax_w",
        "rv32ua-p-amomaxu_w",
        "rv32ua-p-amomin_w",
        "rv32ua-p-amominu_w",
        "rv32ua-p-amoor_w",
        "rv32ua-p-amoswap_w",
        "rv32ua-p-amoxor_w",
        "rv32ua-p-lrsc",
        // rv32mi (16)
        "rv32mi-p-breakpoint",
        "rv32mi-p-csr",
        "rv32mi-p-illegal",
        "rv32mi-p-instret_overflow",
        "rv32mi-p-lh-misaligned",
        "rv32mi-p-lw-misaligned",
        "rv32mi-p-ma_addr",
        "rv32mi-p-ma_fetch",
        "rv32mi-p-mcsr",
        "rv32mi-p-pmpaddr",
        "rv32mi-p-sbreak",
        "rv32mi-p-scall",
        "rv32mi-p-shamt",
        "rv32mi-p-sh-misaligned",
        "rv32mi-p-sw-misaligned",
        "rv32mi-p-zicntr",
        // rv32si (6)
        "rv32si-p-csr",
        "rv32si-p-dirty",
        "rv32si-p-ma_fetch",
        "rv32si-p-sbreak",
        "rv32si-p-scall",
        "rv32si-p-wfi",
    ];

    c.bench_function("all-77-tests", |b| {
        b.iter(|| {
            let mut total_cycles: u64 = 0;
            for bin in &all_bins {
                total_cycles += run_test_return_cycles(bin);
            }
            total_cycles
        });
    });
}

criterion_group!(
    benches,
    bench_rv32ui,
    bench_rv32um,
    bench_rv32ua,
    bench_rv32mi,
    bench_rv32si,
    bench_all_tests,
);
criterion_main!(benches);
