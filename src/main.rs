use std::env;
use std::fs;
use std::io::Write;

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
        eprintln!("Usage: remur <binary_or_elf> [options]");
        eprintln!("  Options:");
        eprintln!("    --tohost <hex_addr>        tohost 地址（raw .bin 用），ELF 自动解析");
        eprintln!("    --signature <output_file>   运行后导出签名区域（arch-test 用）");
        eprintln!("    --cycles <n>                最大执行周期数（默认 10M）");
        eprintln!("  Supports raw .bin and ELF files (auto-detected)");
        std::process::exit(1);
    }

    // 解析命令行参数
    let input_file = &args[1];
    let mut tohost_override: Option<u32> = None;
    let mut signature_file: Option<String> = None;
    let mut max_cycles: u64 = 10_000_000;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--tohost" => {
                i += 1;
                tohost_override = Some(
                    u32::from_str_radix(&args[i], 16).expect("Invalid tohost address")
                );
            }
            "--signature" => {
                i += 1;
                signature_file = Some(args[i].clone());
            }
            "--cycles" => {
                i += 1;
                max_cycles = args[i].parse().expect("Invalid cycle count");
            }
            other => {
                // 向后兼容：第一个未知参数当作 tohost 地址
                if tohost_override.is_none() {
                    tohost_override = Some(
                        u32::from_str_radix(other, 16).expect("Invalid tohost address")
                    );
                }
            }
        }
        i += 1;
    }

    let data = fs::read(input_file).expect("Failed to read input file");
    let mem = memory::Memory::new(MEM_SIZE);
    let mut bus = bus::Bus::new(mem);

    let (entry, tohost, sig_bounds) = if loader::is_elf(&data) {
        let info = loader::load_elf(&data, &mut bus);
        let tohost = tohost_override.or(info.tohost);
        let sig = match (info.begin_signature, info.end_signature) {
            (Some(begin), Some(end)) => Some((begin, end)),
            _ => None,
        };
        (info.entry, tohost, sig)
    } else {
        bus.ram.load_binary(0, &data);
        (bus::RAM_BASE, tohost_override, None)
    };

    bus.tohost_addr = tohost;
    let mut hart = cpu::Hart::new();
    hart.pc = entry;

    match hart.run(&mut bus, max_cycles) {
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
            eprintln!("Timeout after {} cycles", max_cycles);
            eprintln!("PC  = 0x{:08x}", hart.pc);
            eprintln!("a0  = {} (0x{:08x})", hart.read_reg(10), hart.read_reg(10));
            // 签名导出即使超时也尝试
            if let Some(ref path) = signature_file {
                if let Some((begin, end)) = sig_bounds {
                    dump_signature(&bus, begin, end, path);
                }
            }
            std::process::exit(2);
        }
    }

    // 导出签名区域
    if let Some(ref path) = signature_file {
        if let Some((begin, end)) = sig_bounds {
            dump_signature(&bus, begin, end, path);
        } else {
            eprintln!("Warning: --signature specified but ELF has no begin_signature/end_signature symbols");
        }
    }
}

/// 将签名区域按 arch-test 格式导出：每行一个 32-bit word（8 位小写 hex）
fn dump_signature(bus: &bus::Bus, begin: u32, end: u32, path: &str) {
    let mut file = fs::File::create(path)
        .unwrap_or_else(|e| panic!("Cannot create signature file '{}': {}", path, e));

    let mut addr = begin;
    while addr < end {
        let offset = addr.wrapping_sub(bus::RAM_BASE);
        let word = bus.ram.read32(offset);
        writeln!(file, "{:08x}", word)
            .unwrap_or_else(|e| panic!("Write signature failed: {}", e));
        addr += 4;
    }
    eprintln!("Signature dumped: 0x{:08x}..0x{:08x} -> {}", begin, end, path);
}
