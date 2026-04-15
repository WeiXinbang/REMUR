use std::env;
use std::fs;

mod bus;
mod cpu;
mod decode;
mod execute;
mod instruction;
mod memory;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: remur <binary_file> [base_addr_hex] [tohost_addr_hex]");
        eprintln!("  base_addr_hex:   内存基地址，默认 80000000");
        eprintln!("  tohost_addr_hex: tohost 地址（riscv-tests 用），可选");
        std::process::exit(1);
    }

    let binary = fs::read(&args[1]).expect("Failed to read binary file");
    let base: u32 = if args.len() >= 3 {
        u32::from_str_radix(&args[2], 16).expect("Invalid base address")
    } else {
        0x8000_0000
    };
    let tohost: Option<u32> = if args.len() >= 4 {
        Some(u32::from_str_radix(&args[3], 16).expect("Invalid tohost address"))
    } else {
        None
    };

    const MEM_SIZE: usize = 128 * 1024 * 1024;

    let mut mem = memory::Memory::new(MEM_SIZE, base);
    mem.load_binary(base, &binary);

    let mut bus = bus::Bus::new(mem);
    bus.tohost_addr = tohost;

    let mut hart = cpu::Hart::new();
    hart.pc = base;

    match hart.run(&mut bus, 10_000_000) {
        Some(val) => {
            if val == 1 {
                println!("PASS");
            } else {
                let test_num = val >> 1;
                println!("FAIL at test case {}", test_num);
                std::process::exit(1);
            }
        }
        None => {
            println!("Timeout after 10M cycles");
            println!("PC  = 0x{:08x}", hart.pc);
            println!("a0  = {} (0x{:08x})", hart.read_reg(10), hart.read_reg(10));
        }
    }
}
