use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

mod bus;
#[cfg(feature = "cached-decode")]
mod cache;
mod clint;
mod cpu;
mod decode;
mod dtb;
mod execute;
mod instruction;
mod loader;
mod memory;
mod plic;
mod uart;

const MEM_SIZE: usize = 128 * 1024 * 1024;
const DEFAULT_MAX_CYCLES: u64 = 10_000_000;
const DEFAULT_LINUX_MAX_CYCLES: u64 = 200_000_000;
const DEFAULT_KERNEL_ADDR: u32 = 0x8020_0000;
const DEFAULT_DTB_ADDR: u32 = 0x8200_0000;
const DEFAULT_INITRAMFS_ADDR: u32 = 0x8300_0000;
const COUNTER_EN_TM_IR: u32 = (1 << 0) | (1 << 1) | (1 << 2);
const DELEGATE_EXCEPTIONS_TO_S: u32 = (1 << 0) |  // instruction address misaligned
    (1 << 1) |  // instruction access fault
    (1 << 2) |  // illegal instruction
    (1 << 3) |  // breakpoint
    (1 << 4) |  // load address misaligned
    (1 << 5) |  // load access fault
    (1 << 6) |  // store/AMO address misaligned
    (1 << 7) |  // store/AMO access fault
    (1 << 8) |  // ecall from U
    (1 << 9) |  // ecall from S
    (1 << 12) | // instruction page fault
    (1 << 13) | // load page fault
    (1 << 15); // store/AMO page fault

struct NormalOptions {
    input_file: String,
    tohost_override: Option<u32>,
    signature_file: Option<String>,
    max_cycles: u64,
}

struct LinuxOptions {
    kernel_file: Option<String>,
    dtb_file: Option<String>,
    initramfs_file: Option<String>,
    kernel_addr: u32,
    dtb_addr: u32,
    initramfs_addr: u32,
    bootargs: String,
    max_cycles: u64,
}

enum Mode {
    Normal(NormalOptions),
    Linux(LinuxOptions),
}

fn usage_and_exit() -> ! {
    eprintln!("Usage:");
    eprintln!("  remur <binary_or_elf> [options]");
    eprintln!("  remur linux --kernel <image_or_elf> [linux-options]");
    eprintln!("  remur --linux --kernel <image_or_elf> [linux-options]");
    eprintln!();
    eprintln!("Common options:");
    eprintln!(
        "  --cycles <n>                 最大执行周期数（普通模式默认 10M，Linux 模式默认 200M）"
    );
    eprintln!();
    eprintln!("Normal mode options:");
    eprintln!("  --tohost <hex_addr>          tohost 地址（raw .bin 用），ELF 自动解析");
    eprintln!("  --signature <output_file>    运行后导出签名区域（arch-test 用）");
    eprintln!();
    eprintln!("Linux mode options (M6):");
    eprintln!("  --kernel <file>              Linux Image/raw/ELF（可省略：自动下载预构建 Linux）");
    eprintln!("  --dtb <file>                 外部 DTB 文件（不传则内置生成）");
    eprintln!("  --initramfs <file>           initramfs 文件（可选）");
    eprintln!(
        "  --kernel-addr <hex_addr>     raw Image 加载地址（默认 0x80200000；预构建镜像自动对齐到 0x80400000）"
    );
    eprintln!("  --dtb-addr <hex_addr>        DTB 加载地址（默认 0x82000000）");
    eprintln!("  --initramfs-addr <hex_addr>  initramfs 加载地址（默认 0x83000000）");
    eprintln!("  --bootargs <string>          chosen/bootargs");
    std::process::exit(1);
}

fn parse_u32_addr(text: &str, name: &str) -> u32 {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16)
            .unwrap_or_else(|_| panic!("Invalid {}: {}", name, text));
    }
    if t.chars().all(|c| c.is_ascii_hexdigit()) {
        return u32::from_str_radix(t, 16).unwrap_or_else(|_| panic!("Invalid {}: {}", name, text));
    }
    t.parse::<u32>()
        .unwrap_or_else(|_| panic!("Invalid {}: {}", name, text))
}

fn take_next(args: &[String], i: &mut usize, name: &str) -> String {
    *i += 1;
    if *i >= args.len() {
        panic!("Missing value for {}", name);
    }
    args[*i].clone()
}

fn parse_mode(args: &[String]) -> Mode {
    if args.len() < 2 {
        usage_and_exit();
    }

    let linux_mode =
        args.iter().any(|a| a == "--linux") || args.get(1).is_some_and(|a| a == "linux");

    if linux_mode {
        let mut kernel_file: Option<String> = None;
        let mut dtb_file: Option<String> = None;
        let mut initramfs_file: Option<String> = None;
        let mut kernel_addr = DEFAULT_KERNEL_ADDR;
        let mut dtb_addr = DEFAULT_DTB_ADDR;
        let mut initramfs_addr = DEFAULT_INITRAMFS_ADDR;
        let mut bootargs =
            "earlycon=uart8250,mmio,0x10000000 console=ttyS0 rdinit=/bin/sh".to_string();
        let mut max_cycles = DEFAULT_LINUX_MAX_CYCLES;

        let mut i = 1usize;
        while i < args.len() {
            match args[i].as_str() {
                "linux" if i == 1 => {}
                "--linux" => {}
                "--kernel" => kernel_file = Some(take_next(args, &mut i, "--kernel")),
                "--dtb" => dtb_file = Some(take_next(args, &mut i, "--dtb")),
                "--initramfs" => initramfs_file = Some(take_next(args, &mut i, "--initramfs")),
                "--kernel-addr" => {
                    let v = take_next(args, &mut i, "--kernel-addr");
                    kernel_addr = parse_u32_addr(&v, "--kernel-addr");
                }
                "--dtb-addr" => {
                    let v = take_next(args, &mut i, "--dtb-addr");
                    dtb_addr = parse_u32_addr(&v, "--dtb-addr");
                }
                "--initramfs-addr" => {
                    let v = take_next(args, &mut i, "--initramfs-addr");
                    initramfs_addr = parse_u32_addr(&v, "--initramfs-addr");
                }
                "--bootargs" => bootargs = take_next(args, &mut i, "--bootargs"),
                "--cycles" => {
                    let v = take_next(args, &mut i, "--cycles");
                    max_cycles = v
                        .parse::<u64>()
                        .unwrap_or_else(|_| panic!("Invalid --cycles: {}", v));
                }
                other if other.starts_with("--") => {
                    panic!("Unknown option in linux mode: {}", other)
                }
                other => {
                    if kernel_file.is_none() {
                        kernel_file = Some(other.to_string());
                    } else {
                        panic!("Unexpected positional argument: {}", other);
                    }
                }
            }
            i += 1;
        }

        Mode::Linux(LinuxOptions {
            kernel_file,
            dtb_file,
            initramfs_file,
            kernel_addr,
            dtb_addr,
            initramfs_addr,
            bootargs,
            max_cycles,
        })
    } else {
        let mut input_file: Option<String> = None;
        let mut tohost_override: Option<u32> = None;
        let mut signature_file: Option<String> = None;
        let mut max_cycles = DEFAULT_MAX_CYCLES;

        let mut i = 1usize;
        while i < args.len() {
            match args[i].as_str() {
                "--tohost" => {
                    let v = take_next(args, &mut i, "--tohost");
                    tohost_override = Some(parse_u32_addr(&v, "--tohost"));
                }
                "--signature" => {
                    signature_file = Some(take_next(args, &mut i, "--signature"));
                }
                "--cycles" => {
                    let v = take_next(args, &mut i, "--cycles");
                    max_cycles = v
                        .parse::<u64>()
                        .unwrap_or_else(|_| panic!("Invalid --cycles: {}", v));
                }
                "--linux" => panic!("Use --linux with --kernel for Linux boot mode"),
                other if other.starts_with("--") => panic!("Unknown option: {}", other),
                other => {
                    if input_file.is_none() {
                        input_file = Some(other.to_string());
                    } else if tohost_override.is_none() {
                        // 向后兼容：第二个位置参数作为 tohost 地址
                        tohost_override = Some(parse_u32_addr(other, "tohost"));
                    } else {
                        panic!("Unexpected positional argument: {}", other);
                    }
                }
            }
            i += 1;
        }

        let input_file = input_file.unwrap_or_else(|| usage_and_exit());
        Mode::Normal(NormalOptions {
            input_file,
            tohost_override,
            signature_file,
            max_cycles,
        })
    }
}

fn run_normal_mode(opts: NormalOptions) {
    let data = fs::read(&opts.input_file).expect("Failed to read input file");
    let mem = memory::Memory::new(MEM_SIZE);
    let mut bus = bus::Bus::new(mem);

    let (entry, tohost, sig_bounds) = if loader::is_elf(&data) {
        let info = loader::load_elf(&data, &mut bus);
        let tohost = opts.tohost_override.or(info.tohost);
        let sig = match (info.begin_signature, info.end_signature) {
            (Some(begin), Some(end)) => Some((begin, end)),
            _ => None,
        };
        (info.entry, tohost, sig)
    } else {
        bus.ram.load_binary(0, &data);
        (bus::RAM_BASE, opts.tohost_override, None)
    };

    bus.tohost_addr = tohost;
    let mut hart = cpu::Hart::new();
    hart.pc = entry;

    match hart.run(&mut bus, opts.max_cycles) {
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
            eprintln!("Timeout after {} cycles", opts.max_cycles);
            eprintln!("PC  = 0x{:08x}", hart.pc);
            eprintln!("a0  = {} (0x{:08x})", hart.read_reg(10), hart.read_reg(10));
            if let Some(ref path) = opts.signature_file {
                if let Some((begin, end)) = sig_bounds {
                    dump_signature(&bus, begin, end, path);
                }
            }
            std::process::exit(2);
        }
    }

    if let Some(ref path) = opts.signature_file {
        if let Some((begin, end)) = sig_bounds {
            dump_signature(&bus, begin, end, path);
        } else {
            eprintln!(
                "Warning: --signature specified but ELF has no begin_signature/end_signature symbols"
            );
        }
    }
}

fn extract_tar_gz(archive_bytes: &[u8], out_dir: &Path) {
    let gz = flate2::read::GzDecoder::new(archive_bytes);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(out_dir)
        .unwrap_or_else(|e| panic!("Failed to extract linux bundle: {}", e));
}

fn find_linux_bundle_url() -> Result<String, String> {
    let response =
        ureq::get("https://api.github.com/repos/sysprog21/rv32emu-prebuilt/releases?per_page=100")
            .set("User-Agent", "remur-linux-bootstrap")
            .call()
            .map_err(|e| format!("query releases failed: {}", e))?;

    let releases: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("invalid releases JSON: {}", e))?;

    let Some(release_array) = releases.as_array() else {
        return Err("unexpected releases JSON shape".to_string());
    };

    for release in release_array {
        let tag = release["tag_name"].as_str().unwrap_or_default();
        let name = release["name"].as_str().unwrap_or_default();
        if !(tag.contains("Linux-Image") || name.contains("Linux-Image")) {
            continue;
        }
        if let Some(assets) = release["assets"].as_array() {
            for asset in assets {
                let asset_name = asset["name"].as_str().unwrap_or_default();
                if asset_name == "rv32emu-linux-image-prebuilt.tar.gz" {
                    if let Some(url) = asset["browser_download_url"].as_str() {
                        return Ok(url.to_string());
                    }
                }
            }
        }
    }

    Err("no Linux-Image prebuilt asset found in releases".to_string())
}

#[cfg(windows)]
fn download_linux_bundle_via_powershell(out_dir: &Path) -> Result<Vec<u8>, String> {
    let archive_path = out_dir.join("rv32emu-linux-image-prebuilt.tar.gz");
    let archive_escaped = archive_path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop';\
         $ProgressPreference='SilentlyContinue';\
         $releases=Invoke-RestMethod -Headers @{{'User-Agent'='remur-linux-bootstrap'}} -Uri 'https://api.github.com/repos/sysprog21/rv32emu-prebuilt/releases?per_page=100';\
         $asset=$null;\
         foreach($r in $releases){{\
           if(($r.tag_name -like '*Linux-Image*') -or ($r.name -like '*Linux-Image*')){{\
             $asset=$r.assets | Where-Object {{ $_.name -eq 'rv32emu-linux-image-prebuilt.tar.gz' }} | Select-Object -First 1;\
             if($asset){{break}}\
           }}\
         }};\
         if(-not $asset){{throw 'Linux prebuilt asset not found';}};\
         Invoke-WebRequest -Uri $asset.browser_download_url -OutFile '{0}';",
        archive_escaped
    );

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|e| format!("failed to start powershell downloader: {}", e))?;

    if !status.success() {
        return Err("powershell downloader failed".to_string());
    }

    fs::read(&archive_path).map_err(|e| format!("failed to read downloaded archive: {}", e))
}

fn ensure_linux_prebuilt_bundle() -> (String, String) {
    let out_dir = PathBuf::from("target").join("linux-prebuilt");
    let linux_dir = out_dir
        .join("rv32emu-linux-image-prebuilt")
        .join("linux-image");
    let image = linux_dir.join("Image");
    let initrd = linux_dir.join("rootfs.cpio");

    if image.is_file() && initrd.is_file() {
        return (
            image.to_string_lossy().into_owned(),
            initrd.to_string_lossy().into_owned(),
        );
    }

    fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        panic!(
            "Failed to create linux cache dir '{}': {}",
            out_dir.display(),
            e
        )
    });

    let archive = match find_linux_bundle_url() {
        Ok(tar_url) => {
            eprintln!("Fetching Linux prebuilt bundle from {}", tar_url);
            match ureq::get(&tar_url)
                .set("User-Agent", "remur-linux-bootstrap")
                .call()
            {
                Ok(resp) => {
                    let mut archive = Vec::new();
                    let mut reader = resp.into_reader();
                    reader
                        .read_to_end(&mut archive)
                        .unwrap_or_else(|e| panic!("Failed to read linux prebuilt bundle: {}", e));
                    archive
                }
                Err(e) => {
                    #[cfg(windows)]
                    {
                        eprintln!(
                            "Direct TLS download failed ({}), fallback to PowerShell...",
                            e
                        );
                        download_linux_bundle_via_powershell(&out_dir).unwrap_or_else(|pe| {
                            panic!("Failed to download linux prebuilt bundle: {}", pe)
                        })
                    }
                    #[cfg(not(windows))]
                    {
                        panic!("Failed to download linux prebuilt bundle: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            #[cfg(windows)]
            {
                eprintln!(
                    "GitHub API via TLS failed ({}), fallback to PowerShell...",
                    e
                );
                download_linux_bundle_via_powershell(&out_dir)
                    .unwrap_or_else(|pe| panic!("Failed to download linux prebuilt bundle: {}", pe))
            }
            #[cfg(not(windows))]
            {
                panic!("Failed to resolve linux prebuilt bundle URL: {}", e);
            }
        }
    };

    extract_tar_gz(&archive, &out_dir);

    assert!(
        image.is_file() && initrd.is_file(),
        "Prebuilt linux bundle extracted but Image/rootfs.cpio missing under '{}'",
        linux_dir.display()
    );

    (
        image.to_string_lossy().into_owned(),
        initrd.to_string_lossy().into_owned(),
    )
}

fn ram_offset_for_load(addr: u32, low_ram_alias: bool) -> u32 {
    if low_ram_alias && addr < bus::RAM_BASE {
        addr
    } else {
        addr.wrapping_sub(bus::RAM_BASE)
    }
}

fn run_linux_mode(opts: LinuxOptions) {
    let LinuxOptions {
        kernel_file,
        dtb_file,
        initramfs_file,
        mut kernel_addr,
        mut dtb_addr,
        mut initramfs_addr,
        mut bootargs,
        max_cycles,
    } = opts;

    let mem = memory::Memory::new(MEM_SIZE);
    let mut bus = bus::Bus::new(mem);
    let using_prebuilt_bundle = kernel_file.is_none();
    let use_rv32emu_profile = using_prebuilt_bundle
        && env::var("REMUR_LINUX_PROFILE")
            .map(|v| v.eq_ignore_ascii_case("rv32emu"))
            .unwrap_or(false);
    if using_prebuilt_bundle && !use_rv32emu_profile && kernel_addr == DEFAULT_KERNEL_ADDR {
        // Linux Image 头要求 kernel load address 4MiB 对齐，0x8020_0000 会在早期触发 ebreak。
        kernel_addr = 0x8040_0000;
    }
    if use_rv32emu_profile {
        bus.enable_rv32emu_linux_compat();
        if kernel_addr == DEFAULT_KERNEL_ADDR {
            kernel_addr = 0x0000_0000;
        }
        if dtb_addr == DEFAULT_DTB_ADDR {
            dtb_addr = 0x07F0_0000;
        }
        if initramfs_addr == DEFAULT_INITRAMFS_ADDR {
            initramfs_addr = 0x0700_0000;
        }
        if bootargs == "console=ttyS0 root=/dev/ram rw"
            || bootargs == "earlycon=uart8250,mmio,0x10000000 console=ttyS0"
            || bootargs == "earlycon=uart8250,mmio,0x10000000 console=ttyS0 rdinit=/bin/sh"
        {
            bootargs = "earlycon console=ttyS0".to_string();
        }
    }

    let (kernel_path, auto_initramfs) = match kernel_file {
        Some(path) => (path, None),
        None => {
            let (image, initrd) = ensure_linux_prebuilt_bundle();
            (image, Some(initrd))
        }
    };

    let kernel_data = fs::read(&kernel_path)
        .unwrap_or_else(|e| panic!("Failed to read kernel '{}': {}", kernel_path, e));
    let kernel_entry = if loader::is_elf(&kernel_data) {
        let info = loader::load_elf(&kernel_data, &mut bus);
        info.entry
    } else {
        let kernel_offset = ram_offset_for_load(kernel_addr, bus.is_low_ram_alias_enabled());
        bus.ram.load_binary(kernel_offset, &kernel_data);
        kernel_addr
    };

    let initramfs_file = initramfs_file.or(auto_initramfs);
    let initrd_bounds = if let Some(ref initramfs_file) = initramfs_file {
        let initrd = fs::read(initramfs_file)
            .unwrap_or_else(|e| panic!("Failed to read initramfs '{}': {}", initramfs_file, e));
        let offset = ram_offset_for_load(initramfs_addr, bus.is_low_ram_alias_enabled());
        bus.ram.load_binary(offset, &initrd);
        Some((
            initramfs_addr,
            initramfs_addr.wrapping_add(initrd.len() as u32),
        ))
    } else {
        None
    };

    let dtb_blob = if let Some(ref dtb_file) = dtb_file {
        fs::read(dtb_file).unwrap_or_else(|e| panic!("Failed to read DTB '{}': {}", dtb_file, e))
    } else if use_rv32emu_profile {
        dtb::build_rv32emu_compat_dtb(&bootargs, MEM_SIZE as u32, initrd_bounds)
    } else {
        dtb::build_default_dtb(&bootargs, bus::RAM_BASE, MEM_SIZE as u32, initrd_bounds)
    };
    let dtb_offset = ram_offset_for_load(dtb_addr, bus.is_low_ram_alias_enabled());
    bus.ram.load_binary(dtb_offset, &dtb_blob);

    let mut hart = cpu::Hart::new();
    hart.pc = kernel_entry;
    hart.privilege = 1; // Linux 内核运行在 S-mode
    bus.uart.enable_host_input();
    hart.enable_sbi(true);
    hart.write_reg(10, 0); // a0 = hartid
    hart.write_reg(11, dtb_addr); // a1 = FDT physical address
    hart.write_csr(cpu::MEDELEG, DELEGATE_EXCEPTIONS_TO_S);
    hart.write_csr(cpu::MIDELEG, cpu::MIP_SSIP | cpu::MIP_STIP | cpu::MIP_SEIP);
    hart.write_csr(cpu::MCOUNTEREN, COUNTER_EN_TM_IR);
    hart.write_csr(cpu::SCOUNTEREN, COUNTER_EN_TM_IR);

    println!(
        "Linux boot: entry=0x{:08x}, dtb=0x{:08x}, initramfs={}",
        kernel_entry,
        dtb_addr,
        initrd_bounds
            .map(|(s, _)| format!("0x{:08x}", s))
            .unwrap_or_else(|| "none".to_string())
    );

    for _ in 0..max_cycles {
        hart.step(&mut bus);
        if hart.shutdown_requested() {
            println!("SBI shutdown");
            return;
        }
        if hart.pc == 0 {
            eprintln!("Fatal trap to 0 detected");
            break;
        }
    }

    eprintln!("Timeout after {} cycles", max_cycles);
    eprintln!("PC  = 0x{:08x}", hart.pc);
    eprintln!("a0  = {} (0x{:08x})", hart.read_reg(10), hart.read_reg(10));
    eprintln!("priv= {}", hart.privilege);
    eprintln!(
        "mcause=0x{:08x} mepc=0x{:08x} mtval=0x{:08x}",
        hart.read_csr(cpu::MCAUSE),
        hart.read_csr(cpu::MEPC),
        hart.read_csr(cpu::MTVAL)
    );
    eprintln!(
        "scause=0x{:08x} sepc=0x{:08x} stval=0x{:08x}",
        hart.read_csr(cpu::SCAUSE),
        hart.read_csr(cpu::SEPC),
        hart.read_csr(cpu::STVAL)
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mode = parse_mode(&args);
    match mode {
        Mode::Normal(opts) => run_normal_mode(opts),
        Mode::Linux(opts) => run_linux_mode(opts),
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
        writeln!(file, "{:08x}", word).unwrap_or_else(|e| panic!("Write signature failed: {}", e));
        addr += 4;
    }
    eprintln!(
        "Signature dumped: 0x{:08x}..0x{:08x} -> {}",
        begin, end, path
    );
}
