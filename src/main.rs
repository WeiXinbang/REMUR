use std::env;
use std::fs;

mod cpu;
mod decode;
mod execute;
mod instruction;
mod memory;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: remur <binary_file> [base_addr_hex]");
        eprintln!("  base_addr_hex: 内存基地址，默认 80000000");
        std::process::exit(1);
    }

    let binary = fs::read(&args[1]).expect("Failed to read binary file");
    let base: u32 = if args.len() >= 3 {
        u32::from_str_radix(&args[2], 16).expect("Invalid base address")
    } else {
        0x8000_0000
    };

    const MEM_SIZE: usize = 128 * 1024 * 1024; // 128MB

    let mut mem = memory::Memory::new(MEM_SIZE, base);
    mem.load_binary(base, &binary);

    let mut hart = cpu::Hart::new();
    hart.pc = base;

    hart.run(&mut mem, 100_000);

    // 打印最终状态
    println!("PC  = 0x{:08x}", hart.pc);
    println!("a0  = {} (0x{:08x})", hart.read_reg(10), hart.read_reg(10));
    println!("a1  = {} (0x{:08x})", hart.read_reg(11), hart.read_reg(11));
}
