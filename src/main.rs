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
mod loader;
mod memory;
mod plic;
mod uart;

const MEM_SIZE: usize = 128 * 1024 * 1024;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: remur <binary_or_elf> [tohost_addr_hex]");
        eprintln!("  Supports raw .bin and ELF files (auto-detected)");
        eprintln!("  tohost_addr_hex: tohost 地址（raw .bin 用），ELF 自动解析");
        std::process::exit(1);
    }

    let data = fs::read(&args[1]).expect("Failed to read input file");
    let mem = memory::Memory::new(MEM_SIZE);
    let mut bus = bus::Bus::new(mem);

    let (entry, tohost) = if loader::is_elf(&data) {
        let info = loader::load_elf(&data, &mut bus);
        let tohost = if args.len() >= 3 {
            Some(u32::from_str_radix(&args[2], 16).expect("Invalid tohost address"))
        } else {
            info.tohost
        };
        (info.entry, tohost)
    } else {
        bus.ram.load_binary(0, &data);
        let tohost = if args.len() >= 3 {
            Some(u32::from_str_radix(&args[2], 16).expect("Invalid tohost address"))
        } else {
            None
        };
        (bus::RAM_BASE, tohost)
    };

    bus.tohost_addr = tohost;
    let mut hart = cpu::Hart::new();
    hart.pc = entry;

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
