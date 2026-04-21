//! Linux 启动辅助：
//! - 预构建 bundle 的发现 / 下载 / 解压
//! - 启动地址与 profile（默认 / rv32emu）的布局修正
//! - kernel/initramfs/DTB 加载与最终运行方式选择

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::process::Command;

use crate::bus::{self, Bus};
use crate::cpu::{self, Hart};
use crate::debug_trace::{DebugRuntime, DifftestContext, prepare_difftest_ref};
use crate::dtb;
use crate::loader;
use crate::memory;
#[cfg(feature = "tui")]
use crate::tui;
use crate::{
    COUNTER_EN_TM_IR, DEFAULT_DTB_ADDR, DEFAULT_INITRAMFS_ADDR, DEFAULT_KERNEL_ADDR,
    DEFAULT_LINUX_BOOTARGS, DEFAULT_LINUX_MAX_CYCLES, DELEGATE_EXCEPTIONS_TO_S, LinuxOptions,
    MEM_SIZE, resolve_tui_cycles,
};

const PREBUILT_RELEASES_URL: &str =
    "https://api.github.com/repos/sysprog21/rv32emu-prebuilt/releases?per_page=100";
const PREBUILT_USER_AGENT: &str = "remur-linux-bootstrap";
const PREBUILT_ARCHIVE_NAME: &str = "rv32emu-linux-image-prebuilt.tar.gz";
const PREBUILT_KERNEL_PATH: &str = "rv32emu-linux-image-prebuilt/linux-image/Image";
const PREBUILT_INITRD_PATH: &str = "rv32emu-linux-image-prebuilt/linux-image/rootfs.cpio";
const PREBUILT_ALIGNED_KERNEL_ADDR: u32 = 0x8040_0000;
const RV32EMU_PROFILE_NAME: &str = "rv32emu";
const RV32EMU_KERNEL_ADDR: u32 = 0x0000_0000;
const RV32EMU_DTB_ADDR: u32 = 0x07F0_0000;
const RV32EMU_INITRAMFS_ADDR: u32 = 0x0700_0000;
const RV32EMU_BOOTARGS: &str = "earlycon console=ttyS0";

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinuxBootLayout {
    kernel_addr: u32,
    dtb_addr: u32,
    initramfs_addr: u32,
    bootargs: String,
    use_rv32emu_profile: bool,
}

impl LinuxBootLayout {
    /// Linux 模式的“布局决策”集中在这里：
    /// 这样 profile 切换和默认地址修正都不会散落在运行主流程里。
    fn resolve(
        using_prebuilt_bundle: bool,
        kernel_addr: u32,
        dtb_addr: u32,
        initramfs_addr: u32,
        bootargs: String,
        requested_profile: Option<&str>,
    ) -> Self {
        let use_rv32emu_profile = using_prebuilt_bundle
            && requested_profile
                .map(|v| v.eq_ignore_ascii_case(RV32EMU_PROFILE_NAME))
                .unwrap_or(false);

        if use_rv32emu_profile {
            return Self {
                kernel_addr: if kernel_addr == DEFAULT_KERNEL_ADDR {
                    RV32EMU_KERNEL_ADDR
                } else {
                    kernel_addr
                },
                dtb_addr: if dtb_addr == DEFAULT_DTB_ADDR {
                    RV32EMU_DTB_ADDR
                } else {
                    dtb_addr
                },
                initramfs_addr: if initramfs_addr == DEFAULT_INITRAMFS_ADDR {
                    RV32EMU_INITRAMFS_ADDR
                } else {
                    initramfs_addr
                },
                bootargs: normalize_rv32emu_bootargs(&bootargs),
                use_rv32emu_profile: true,
            };
        }

        Self {
            kernel_addr: if using_prebuilt_bundle && kernel_addr == DEFAULT_KERNEL_ADDR {
                PREBUILT_ALIGNED_KERNEL_ADDR
            } else {
                kernel_addr
            },
            dtb_addr,
            initramfs_addr,
            bootargs,
            use_rv32emu_profile: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinuxArtifacts {
    kernel_path: String,
    kernel_entry: u32,
    dtb_addr: u32,
    bootargs: String,
    initrd_bounds: Option<(u32, u32)>,
}

fn normalize_rv32emu_bootargs(bootargs: &str) -> String {
    if bootargs == "console=ttyS0 root=/dev/ram rw"
        || bootargs == "earlycon=uart8250,mmio,0x10000000 console=ttyS0"
        || bootargs == DEFAULT_LINUX_BOOTARGS
    {
        RV32EMU_BOOTARGS.to_string()
    } else {
        bootargs.to_string()
    }
}

fn linux_prebuilt_cache_dir() -> PathBuf {
    PathBuf::from("target").join("linux-prebuilt")
}

fn linux_prebuilt_kernel_path(out_dir: &Path) -> PathBuf {
    out_dir.join(PREBUILT_KERNEL_PATH)
}

fn linux_prebuilt_initrd_path(out_dir: &Path) -> PathBuf {
    out_dir.join(PREBUILT_INITRD_PATH)
}

fn extract_tar_gz(archive_bytes: &[u8], out_dir: &Path) {
    let gz = flate2::read::GzDecoder::new(archive_bytes);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(out_dir)
        .unwrap_or_else(|e| panic!("Failed to extract linux bundle: {}", e));
}

fn find_linux_bundle_url() -> Result<String, String> {
    let response = ureq::get(PREBUILT_RELEASES_URL)
        .set("User-Agent", PREBUILT_USER_AGENT)
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
                if asset_name == PREBUILT_ARCHIVE_NAME
                    && let Some(url) = asset["browser_download_url"].as_str()
                {
                    return Ok(url.to_string());
                }
            }
        }
    }

    Err("no Linux-Image prebuilt asset found in releases".to_string())
}

#[cfg(windows)]
fn download_linux_bundle_via_powershell(out_dir: &Path) -> Result<Vec<u8>, String> {
    let archive_path = out_dir.join(PREBUILT_ARCHIVE_NAME);
    let archive_escaped = archive_path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop';\
         $ProgressPreference='SilentlyContinue';\
         $releases=Invoke-RestMethod -Headers @{{'User-Agent'='{0}'}} -Uri '{1}';\
         $asset=$null;\
         foreach($r in $releases){{\
           if(($r.tag_name -like '*Linux-Image*') -or ($r.name -like '*Linux-Image*')){{\
             $asset=$r.assets | Where-Object {{ $_.name -eq '{2}' }} | Select-Object -First 1;\
             if($asset){{break}}\
           }}\
         }};\
         if(-not $asset){{throw 'Linux prebuilt asset not found';}};\
         Invoke-WebRequest -Uri $asset.browser_download_url -OutFile '{3}';",
        PREBUILT_USER_AGENT, PREBUILT_RELEASES_URL, PREBUILT_ARCHIVE_NAME, archive_escaped
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
    let out_dir = linux_prebuilt_cache_dir();
    let image = linux_prebuilt_kernel_path(&out_dir);
    let initrd = linux_prebuilt_initrd_path(&out_dir);

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
                .set("User-Agent", PREBUILT_USER_AGENT)
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
        out_dir.display()
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

fn resolve_kernel_source(kernel_file: Option<String>) -> (String, Option<String>) {
    match kernel_file {
        Some(path) => (path, None),
        None => {
            let (image, initrd) = ensure_linux_prebuilt_bundle();
            (image, Some(initrd))
        }
    }
}

fn load_kernel_into_bus(bus: &mut Bus, kernel_path: &str, kernel_addr: u32) -> u32 {
    let kernel_data = fs::read(kernel_path)
        .unwrap_or_else(|e| panic!("Failed to read kernel '{}': {}", kernel_path, e));
    if loader::is_elf(&kernel_data) {
        let info = loader::load_elf(&kernel_data, bus);
        info.entry
    } else {
        let kernel_offset = ram_offset_for_load(kernel_addr, bus.is_low_ram_alias_enabled());
        bus.ram.load_binary(kernel_offset, &kernel_data);
        kernel_addr
    }
}

fn load_initramfs_into_bus(
    bus: &mut Bus,
    initramfs_file: Option<String>,
    initramfs_addr: u32,
) -> Option<(u32, u32)> {
    let initramfs_file = initramfs_file?;
    let initrd = fs::read(&initramfs_file)
        .unwrap_or_else(|e| panic!("Failed to read initramfs '{}': {}", initramfs_file, e));
    let offset = ram_offset_for_load(initramfs_addr, bus.is_low_ram_alias_enabled());
    bus.ram.load_binary(offset, &initrd);
    Some((
        initramfs_addr,
        initramfs_addr.wrapping_add(initrd.len() as u32),
    ))
}

fn build_dtb_blob(
    dtb_file: &Option<String>,
    bootargs: &str,
    use_rv32emu_profile: bool,
    initrd_bounds: Option<(u32, u32)>,
) -> Vec<u8> {
    if let Some(dtb_file) = dtb_file {
        fs::read(dtb_file).unwrap_or_else(|e| panic!("Failed to read DTB '{}': {}", dtb_file, e))
    } else if use_rv32emu_profile {
        dtb::build_rv32emu_compat_dtb(bootargs, MEM_SIZE as u32, initrd_bounds)
    } else {
        dtb::build_default_dtb(bootargs, bus::RAM_BASE, MEM_SIZE as u32, initrd_bounds)
    }
}

fn prepare_linux_artifacts(bus: &mut Bus, opts: LinuxOptions) -> LinuxArtifacts {
    let LinuxOptions {
        kernel_file,
        dtb_file,
        initramfs_file,
        kernel_addr,
        dtb_addr,
        initramfs_addr,
        bootargs,
        ..
    } = opts;

    let using_prebuilt_bundle = kernel_file.is_none();
    let layout = LinuxBootLayout::resolve(
        using_prebuilt_bundle,
        kernel_addr,
        dtb_addr,
        initramfs_addr,
        bootargs,
        env::var("REMUR_LINUX_PROFILE").ok().as_deref(),
    );

    if layout.use_rv32emu_profile {
        bus.enable_rv32emu_linux_compat();
    }

    let (kernel_path, auto_initramfs) = resolve_kernel_source(kernel_file);
    let kernel_entry = load_kernel_into_bus(bus, &kernel_path, layout.kernel_addr);
    let initrd_bounds = load_initramfs_into_bus(
        bus,
        initramfs_file.or(auto_initramfs),
        layout.initramfs_addr,
    );
    let dtb_blob = build_dtb_blob(
        &dtb_file,
        &layout.bootargs,
        layout.use_rv32emu_profile,
        initrd_bounds,
    );
    let dtb_offset = ram_offset_for_load(layout.dtb_addr, bus.is_low_ram_alias_enabled());
    bus.ram.load_binary(dtb_offset, &dtb_blob);

    LinuxArtifacts {
        kernel_path,
        kernel_entry,
        dtb_addr: layout.dtb_addr,
        bootargs: layout.bootargs,
        initrd_bounds,
    }
}

fn configure_linux_hart(
    hart: &mut Hart,
    bus: &mut Bus,
    kernel_entry: u32,
    dtb_addr: u32,
    use_tui: bool,
) {
    hart.pc = kernel_entry;
    hart.privilege = 1; // Linux 内核运行在 S-mode
    if !use_tui {
        bus.uart.enable_host_input();
    }
    hart.enable_sbi(true);
    hart.write_reg(10, 0); // a0 = hartid
    hart.write_reg(11, dtb_addr); // a1 = FDT physical address
    hart.write_csr(cpu::MEDELEG, DELEGATE_EXCEPTIONS_TO_S);
    hart.write_csr(cpu::MIDELEG, cpu::MIP_SSIP | cpu::MIP_STIP | cpu::MIP_SEIP);
    hart.write_csr(cpu::MCOUNTEREN, COUNTER_EN_TM_IR);
    hart.write_csr(cpu::SCOUNTEREN, COUNTER_EN_TM_IR);
}

fn print_linux_boot_banner(artifacts: &LinuxArtifacts) {
    println!(
        "Linux boot: entry=0x{:08x}, dtb=0x{:08x}, initramfs={}",
        artifacts.kernel_entry,
        artifacts.dtb_addr,
        artifacts
            .initrd_bounds
            .map(|(s, _)| format!("0x{:08x}", s))
            .unwrap_or_else(|| "none".to_string())
    );
}

fn maybe_run_linux_tui(use_tui: bool, max_cycles: u64, hart: &mut Hart, bus: &mut Bus) -> bool {
    #[cfg(feature = "tui")]
    if use_tui {
        bus.uart.enable_capture();
        let tui_cycles = resolve_tui_cycles(max_cycles, DEFAULT_LINUX_MAX_CYCLES);
        tui::run_tui_linux(hart, bus, tui_cycles);
        return true;
    }

    #[cfg(not(feature = "tui"))]
    if use_tui {
        let _ = (max_cycles, hart, bus);
        panic!("TUI 功能未编译，请使用 --features tui 重新构建");
    }

    false
}

fn run_linux_headless(
    hart: &mut Hart,
    bus: &mut Bus,
    max_cycles: u64,
    kernel_path: &str,
    bootargs: &str,
    mut debug: crate::DebugOptions,
) {
    if let Err(e) = prepare_difftest_ref(
        &mut debug,
        &DifftestContext {
            mode: "linux",
            workload: kernel_path,
            kernel: Some(kernel_path),
            bootargs: Some(bootargs),
        },
    ) {
        panic!("{e}");
    }

    let mut debug_runtime = if debug.needs_step_snapshots() {
        Some(DebugRuntime::new(debug).unwrap_or_else(|e| panic!("{e}")))
    } else {
        None
    };

    for cycle in 0..max_cycles {
        if let Some(runtime) = debug_runtime.as_mut() {
            let snapshot = hart.step_snapshot(bus);
            if let Err(e) = runtime.observe_step(cycle, &snapshot) {
                eprintln!("{e}");
                std::process::exit(3);
            }
        } else {
            hart.step(bus);
        }
        if hart.shutdown_requested() {
            if let Some(runtime) = debug_runtime.as_ref()
                && let Err(e) = runtime.finalize()
            {
                eprintln!("{e}");
                std::process::exit(3);
            }
            println!("SBI shutdown");
            return;
        }
        if hart.pc == 0 {
            eprintln!("Fatal trap to 0 detected");
            break;
        }
    }

    if let Some(runtime) = debug_runtime.as_ref()
        && let Err(e) = runtime.finalize()
    {
        eprintln!("{e}");
        std::process::exit(3);
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

/// Linux 启动入口：负责预构建 bundle、地址布局、镜像加载和最终运行方式选择。
pub(crate) fn run_linux_mode(opts: LinuxOptions) {
    let mem = memory::Memory::new(MEM_SIZE);
    let mut bus = Bus::new(mem);
    let max_cycles = opts.max_cycles;
    let use_tui = opts.tui;
    let debug = opts.debug.clone();

    let artifacts = prepare_linux_artifacts(&mut bus, opts);
    let mut hart = Hart::new();
    configure_linux_hart(
        &mut hart,
        &mut bus,
        artifacts.kernel_entry,
        artifacts.dtb_addr,
        use_tui,
    );
    print_linux_boot_banner(&artifacts);

    if maybe_run_linux_tui(use_tui, max_cycles, &mut hart, &mut bus) {
        return;
    }

    run_linux_headless(
        &mut hart,
        &mut bus,
        max_cycles,
        &artifacts.kernel_path,
        &artifacts.bootargs,
        debug,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prebuilt_linux_layout_aligns_default_kernel_addr() {
        let layout = LinuxBootLayout::resolve(
            true,
            DEFAULT_KERNEL_ADDR,
            DEFAULT_DTB_ADDR,
            DEFAULT_INITRAMFS_ADDR,
            DEFAULT_LINUX_BOOTARGS.to_string(),
            None,
        );

        assert_eq!(layout.kernel_addr, PREBUILT_ALIGNED_KERNEL_ADDR);
        assert_eq!(layout.dtb_addr, DEFAULT_DTB_ADDR);
        assert_eq!(layout.initramfs_addr, DEFAULT_INITRAMFS_ADDR);
        assert_eq!(layout.bootargs, DEFAULT_LINUX_BOOTARGS);
        assert!(!layout.use_rv32emu_profile);
    }

    #[test]
    fn rv32emu_profile_rewrites_default_addrs_and_bootargs() {
        let layout = LinuxBootLayout::resolve(
            true,
            DEFAULT_KERNEL_ADDR,
            DEFAULT_DTB_ADDR,
            DEFAULT_INITRAMFS_ADDR,
            DEFAULT_LINUX_BOOTARGS.to_string(),
            Some("rv32emu"),
        );

        assert_eq!(layout.kernel_addr, RV32EMU_KERNEL_ADDR);
        assert_eq!(layout.dtb_addr, RV32EMU_DTB_ADDR);
        assert_eq!(layout.initramfs_addr, RV32EMU_INITRAMFS_ADDR);
        assert_eq!(layout.bootargs, RV32EMU_BOOTARGS);
        assert!(layout.use_rv32emu_profile);
    }

    #[test]
    fn rv32emu_profile_keeps_custom_bootargs() {
        let layout = LinuxBootLayout::resolve(
            true,
            DEFAULT_KERNEL_ADDR,
            DEFAULT_DTB_ADDR,
            DEFAULT_INITRAMFS_ADDR,
            "console=ttyS0 custom=1".to_string(),
            Some("rv32emu"),
        );

        assert_eq!(layout.bootargs, "console=ttyS0 custom=1");
    }

    #[test]
    fn low_ram_alias_uses_identity_offset() {
        assert_eq!(ram_offset_for_load(0x1000, true), 0x1000);
        assert_eq!(ram_offset_for_load(bus::RAM_BASE + 0x1000, true), 0x1000);
        assert_eq!(ram_offset_for_load(bus::RAM_BASE + 0x2000, false), 0x2000);
    }
}
