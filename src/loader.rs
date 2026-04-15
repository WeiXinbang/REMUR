use crate::bus::{Bus, RAM_BASE};
use goblin::elf::Elf;
use goblin::elf::program_header::PT_LOAD;

/// 检测文件是否为 ELF 格式
pub fn is_elf(data: &[u8]) -> bool {
    data.len() >= 4 && data[..4] == [0x7f, b'E', b'L', b'F']
}

/// ELF 加载结果
pub struct ElfInfo {
    pub entry: u32,
    pub tohost: Option<u32>,
}

/// 加载 ELF 文件：解析 PT_LOAD 段到物理内存，返回入口地址和 tohost 符号地址
pub fn load_elf(data: &[u8], bus: &mut Bus) -> ElfInfo {
    let elf = Elf::parse(data).expect("Failed to parse ELF file");

    for ph in elf.program_headers.iter().filter(|ph| ph.p_type == PT_LOAD) {
        let paddr = ph.p_paddr as u32;
        let file_range = ph.file_range();
        let memsz = ph.p_memsz as usize;

        if !file_range.is_empty() {
            let offset = paddr.wrapping_sub(RAM_BASE);
            let file_data = &data[file_range];
            bus.ram.load_binary(offset, file_data);
            // BSS: zero-fill remaining bytes
            if memsz > file_data.len() {
                let bss_start = offset as usize + file_data.len();
                let bss_end = offset as usize + memsz;
                for i in bss_start..bss_end {
                    bus.ram.write8(i as u32, 0);
                }
            }
        }
    }

    let tohost = elf.syms.iter()
        .find(|sym| {
            elf.strtab.get_at(sym.st_name).map_or(false, |name| name == "tohost")
        })
        .map(|sym| sym.st_value as u32);

    ElfInfo { entry: elf.entry as u32, tohost }
}
