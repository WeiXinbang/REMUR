use std::collections::HashMap;

const FDT_MAGIC: u32 = 0xD00D_FEED;
const FDT_BEGIN_NODE: u32 = 0x0000_0001;
const FDT_END_NODE: u32 = 0x0000_0002;
const FDT_PROP: u32 = 0x0000_0003;
const FDT_END: u32 = 0x0000_0009;

fn push_be32(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn align4(buf: &mut Vec<u8>) {
    while (buf.len() & 3) != 0 {
        buf.push(0);
    }
}

struct DtbBuilder {
    struct_block: Vec<u8>,
    strings: Vec<u8>,
    string_offsets: HashMap<String, u32>,
}

impl DtbBuilder {
    fn new() -> Self {
        Self {
            struct_block: Vec::new(),
            strings: Vec::new(),
            string_offsets: HashMap::new(),
        }
    }

    fn string_offset(&mut self, name: &str) -> u32 {
        if let Some(off) = self.string_offsets.get(name) {
            return *off;
        }
        let off = self.strings.len() as u32;
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);
        self.string_offsets.insert(name.to_string(), off);
        off
    }

    fn begin_node(&mut self, name: &str) {
        push_be32(&mut self.struct_block, FDT_BEGIN_NODE);
        self.struct_block.extend_from_slice(name.as_bytes());
        self.struct_block.push(0);
        align4(&mut self.struct_block);
    }

    fn end_node(&mut self) {
        push_be32(&mut self.struct_block, FDT_END_NODE);
    }

    fn prop_raw(&mut self, name: &str, data: &[u8]) {
        let name_off = self.string_offset(name);
        push_be32(&mut self.struct_block, FDT_PROP);
        push_be32(&mut self.struct_block, data.len() as u32);
        push_be32(&mut self.struct_block, name_off);
        self.struct_block.extend_from_slice(data);
        align4(&mut self.struct_block);
    }

    fn prop_empty(&mut self, name: &str) {
        self.prop_raw(name, &[]);
    }

    fn prop_u32(&mut self, name: &str, value: u32) {
        self.prop_raw(name, &value.to_be_bytes());
    }

    fn prop_u32_list(&mut self, name: &str, values: &[u32]) {
        let mut data = Vec::with_capacity(values.len() * 4);
        for &v in values {
            data.extend_from_slice(&v.to_be_bytes());
        }
        self.prop_raw(name, &data);
    }

    fn prop_str(&mut self, name: &str, value: &str) {
        let mut data = Vec::with_capacity(value.len() + 1);
        data.extend_from_slice(value.as_bytes());
        data.push(0);
        self.prop_raw(name, &data);
    }

    fn finish(mut self) -> Vec<u8> {
        push_be32(&mut self.struct_block, FDT_END);

        let header_size = 10 * 4;
        let mem_rsvmap_size = 16; // 仅终止项：address=0,size=0
        let off_mem_rsvmap = header_size as u32;
        let off_dt_struct = off_mem_rsvmap + mem_rsvmap_size as u32;
        let off_dt_strings = off_dt_struct + self.struct_block.len() as u32;
        let totalsize = off_dt_strings + self.strings.len() as u32;

        let mut out = Vec::with_capacity(totalsize as usize);
        push_be32(&mut out, FDT_MAGIC);
        push_be32(&mut out, totalsize);
        push_be32(&mut out, off_dt_struct);
        push_be32(&mut out, off_dt_strings);
        push_be32(&mut out, off_mem_rsvmap);
        push_be32(&mut out, 17); // version
        push_be32(&mut out, 16); // last compatible version
        push_be32(&mut out, 0); // boot_cpuid_phys
        push_be32(&mut out, self.strings.len() as u32);
        push_be32(&mut out, self.struct_block.len() as u32);

        // mem reserve map terminator (u64 addr, u64 size) = 0
        out.extend_from_slice(&[0u8; 16]);
        out.extend_from_slice(&self.struct_block);
        out.extend_from_slice(&self.strings);
        out
    }
}

/// 生成可用于 Linux 启动的最小 DTB（virt 风格内存映射）
pub fn build_default_dtb(
    bootargs: &str,
    mem_base: u32,
    mem_size: u32,
    initrd: Option<(u32, u32)>,
) -> Vec<u8> {
    const CPU_INTC_PHANDLE: u32 = 1;
    const PLIC_PHANDLE: u32 = 2;

    let mut b = DtbBuilder::new();

    b.begin_node("");
    b.prop_u32("#address-cells", 2);
    b.prop_u32("#size-cells", 2);
    b.prop_str("compatible", "riscv-virtio");
    b.prop_str("model", "remur-rv32ima");

    b.begin_node("aliases");
    b.prop_str("serial0", "/soc/uart@10000000");
    b.end_node();

    b.begin_node("chosen");
    b.prop_str("bootargs", bootargs);
    b.prop_str("stdout-path", "serial0:115200n8");
    if let Some((start, end)) = initrd {
        b.prop_u32("linux,initrd-start", start);
        b.prop_u32("linux,initrd-end", end);
    }
    b.end_node();

    b.begin_node("cpus");
    b.prop_u32("#address-cells", 1);
    b.prop_u32("#size-cells", 0);
    b.prop_u32("timebase-frequency", 65_000_000);

    b.begin_node("cpu@0");
    b.prop_str("device_type", "cpu");
    b.prop_u32("reg", 0);
    b.prop_str("status", "okay");
    b.prop_str("compatible", "riscv");
    b.prop_str("riscv,isa", "rv32ima_zicsr_zifencei_zicntr");
    b.prop_str("mmu-type", "riscv,sv32");
    b.prop_u32("clock-frequency", 65_000_000);

    b.begin_node("interrupt-controller");
    b.prop_u32("#interrupt-cells", 1);
    b.prop_empty("interrupt-controller");
    b.prop_str("compatible", "riscv,cpu-intc");
    b.prop_u32("phandle", CPU_INTC_PHANDLE);
    b.end_node();

    b.end_node();
    b.end_node();

    b.begin_node("memory@80000000");
    b.prop_str("device_type", "memory");
    b.prop_u32_list("reg", &[0, mem_base, 0, mem_size]);
    b.end_node();

    b.begin_node("soc");
    b.prop_u32("#address-cells", 2);
    b.prop_u32("#size-cells", 2);
    b.prop_str("compatible", "simple-bus");
    b.prop_empty("ranges");

    b.begin_node("clint@2000000");
    b.prop_str("compatible", "riscv,clint0");
    b.prop_u32_list("reg", &[0, 0x0200_0000, 0, 0x0001_0000]);
    b.prop_u32_list(
        "interrupts-extended",
        &[CPU_INTC_PHANDLE, 3, CPU_INTC_PHANDLE, 7],
    );
    b.end_node();

    b.begin_node("plic@c000000");
    b.prop_str("compatible", "sifive,plic-1.0.0");
    b.prop_u32_list("reg", &[0, 0x0C00_0000, 0, 0x0400_0000]);
    b.prop_u32("riscv,ndev", 31);
    b.prop_u32("phandle", PLIC_PHANDLE);
    b.prop_u32("#interrupt-cells", 1);
    b.prop_empty("interrupt-controller");
    b.prop_u32_list(
        "interrupts-extended",
        &[CPU_INTC_PHANDLE, 11, CPU_INTC_PHANDLE, 9],
    );
    b.end_node();

    b.begin_node("uart@10000000");
    b.prop_str("compatible", "ns16550a");
    b.prop_u32_list("reg", &[0, 0x1000_0000, 0, 0x0000_1000]);
    b.prop_u32("clock-frequency", 3_686_400);
    b.prop_u32("current-speed", 115_200);
    b.prop_u32("reg-shift", 0);
    b.prop_u32("reg-io-width", 1);
    b.prop_u32("interrupt-parent", PLIC_PHANDLE);
    b.prop_u32("interrupts", 10);
    b.end_node();

    b.end_node();
    b.end_node();

    b.finish()
}

/// 生成与 rv32emu Linux 预构建镜像兼容的 DTB。
pub fn build_rv32emu_compat_dtb(
    bootargs: &str,
    mem_size: u32,
    initrd: Option<(u32, u32)>,
) -> Vec<u8> {
    const CPU_INTC_PHANDLE: u32 = 1;
    const PLIC_PHANDLE: u32 = 2;
    const SOC_BASE: u32 = 0xF000_0000;

    let mut b = DtbBuilder::new();

    b.begin_node("");
    b.prop_u32("#address-cells", 1);
    b.prop_u32("#size-cells", 1);
    b.prop_str("model", "rv32emu");

    b.begin_node("aliases");
    b.prop_str("serial0", "/soc@f0000000/serial@4000000");
    b.end_node();

    b.begin_node("chosen");
    b.prop_str("bootargs", bootargs);
    b.prop_str("stdout-path", "serial0");
    if let Some((start, end)) = initrd {
        b.prop_u32("linux,initrd-start", start);
        b.prop_u32("linux,initrd-end", end);
    }
    b.end_node();

    b.begin_node("cpus");
    b.prop_u32("#address-cells", 1);
    b.prop_u32("#size-cells", 0);
    b.prop_u32("timebase-frequency", 65_000_000);

    b.begin_node("cpu@0");
    b.prop_str("device_type", "cpu");
    b.prop_str("compatible", "riscv");
    b.prop_u32("reg", 0);
    b.prop_str("riscv,isa", "rv32ima_zicsr_zifencei_zicntr");
    b.prop_str("mmu-type", "riscv,sv32");

    b.begin_node("interrupt-controller");
    b.prop_u32("#interrupt-cells", 1);
    b.prop_u32("#address-cells", 0);
    b.prop_empty("interrupt-controller");
    b.prop_str("compatible", "riscv,cpu-intc");
    b.prop_u32("phandle", CPU_INTC_PHANDLE);
    b.end_node();

    b.end_node();
    b.end_node();

    b.begin_node("memory@0");
    b.prop_str("device_type", "memory");
    b.prop_u32_list("reg", &[0, mem_size]);
    b.end_node();

    b.begin_node("soc@f0000000");
    b.prop_u32("#address-cells", 1);
    b.prop_u32("#size-cells", 1);
    b.prop_str("compatible", "simple-bus");
    b.prop_u32_list("ranges", &[0, SOC_BASE, 0x1000_0000]);
    b.prop_u32("interrupt-parent", PLIC_PHANDLE);

    b.begin_node("interrupt-controller@0");
    b.prop_u32("#interrupt-cells", 1);
    b.prop_u32("#address-cells", 0);
    b.prop_str("compatible", "sifive,plic-1.0.0");
    b.prop_u32_list("reg", &[0x0000_0000, 0x0400_0000]);
    b.prop_empty("interrupt-controller");
    b.prop_u32_list("interrupts-extended", &[CPU_INTC_PHANDLE, 9]);
    b.prop_u32("riscv,ndev", 31);
    b.prop_u32("phandle", PLIC_PHANDLE);
    b.end_node();

    b.begin_node("serial@4000000");
    b.prop_str("compatible", "ns16550");
    b.prop_u32_list("reg", &[0x0400_0000, 0x0010_0000]);
    b.prop_u32("interrupts", 1);
    b.prop_empty("no-loopback-test");
    b.prop_u32("clock-frequency", 5_000_000);
    b.end_node();

    b.end_node();
    b.end_node();

    b.finish()
}
