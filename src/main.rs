use std::env;
use std::fs;

mod bus;
#[cfg(feature = "cached-decode")]
mod cache;
mod clint;
mod cpu;
mod decode;
mod execute;
mod instruction;
mod memory;
mod plic;
mod uart;

const MEM_SIZE: usize = 128 * 1024 * 1024;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: remur <binary_file> [tohost_addr_hex]");
        eprintln!("  tohost_addr_hex: tohost 地址（riscv-tests 用），可选");
        std::process::exit(1);
    }

    let binary = fs::read(&args[1]).expect("Failed to read binary file");
    let tohost: Option<u32> = if args.len() >= 3 {
        Some(u32::from_str_radix(&args[2], 16).expect("Invalid tohost address"))
    } else {
        None
    };

    let mut mem = memory::Memory::new(MEM_SIZE);
    mem.load_binary(0, &binary);

    let mut bus = bus::Bus::new(mem);
    bus.tohost_addr = tohost;

    let mut hart = cpu::Hart::new();
    hart.pc = bus::RAM_BASE;

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
