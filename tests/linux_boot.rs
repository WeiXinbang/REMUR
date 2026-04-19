//! Linux 启动路径最小闭环测试：
//! - S-mode 启动入口
//! - a0/a1 传参（hartid + dtb 指针）
//! - 内嵌 SBI ecall 处理（putchar + shutdown）

use remur::bus::{Bus, RAM_BASE};
use remur::cpu::{Hart, MCOUNTEREN, MIDELEG, MIP_SEIP, MIP_SSIP, MIP_STIP, SCOUNTEREN};
use remur::dtb;
use remur::memory::Memory;

const MEM_SIZE: usize = 128 * 1024 * 1024;
const KERNEL_ADDR: u32 = 0x8020_0000;
const DTB_ADDR: u32 = 0x8200_0000;

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xFFF;
    (imm << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

fn ecall() -> u32 {
    0x0000_0073
}

fn encode_kernel() -> Vec<u32> {
    vec![
        // a0 = 'L', a7 = 1  => legacy console_putchar
        addi(10, 0, 'L' as i32),
        addi(17, 0, 1),
        ecall(),
        // a7 = 8 => legacy shutdown
        addi(17, 0, 8),
        ecall(),
        // 如果 shutdown 没生效会跑到这里（保持可重复执行）
        addi(0, 0, 0),
    ]
}

#[test]
fn linux_entry_and_sbi_shutdown_smoke() {
    let mem = Memory::new(MEM_SIZE);
    let mut bus = Bus::new(mem);

    // 模拟 Linux raw Image 加载
    for (i, inst) in encode_kernel().iter().enumerate() {
        let offset = KERNEL_ADDR.wrapping_sub(RAM_BASE) + (i as u32) * 4;
        bus.ram.write32(offset, *inst);
    }

    // 模拟 DTB 加载
    let dtb_blob = dtb::build_default_dtb("console=ttyS0", RAM_BASE, MEM_SIZE as u32, None);
    let dtb_off = DTB_ADDR.wrapping_sub(RAM_BASE);
    bus.ram.load_binary(dtb_off, &dtb_blob);

    // 验证 DTB magic（大端 D00DFEED）
    assert_eq!(bus.ram.read8(dtb_off), 0xD0);
    assert_eq!(bus.ram.read8(dtb_off + 1), 0x0D);
    assert_eq!(bus.ram.read8(dtb_off + 2), 0xFE);
    assert_eq!(bus.ram.read8(dtb_off + 3), 0xED);

    let mut hart = Hart::new();
    hart.pc = KERNEL_ADDR;
    hart.privilege = 1; // S-mode
    hart.enable_sbi(true);
    hart.write_reg(10, 0); // a0 = hartid
    hart.write_reg(11, DTB_ADDR); // a1 = FDT pointer
    hart.write_csr(MIDELEG, MIP_SSIP | MIP_STIP | MIP_SEIP);
    hart.write_csr(MCOUNTEREN, (1 << 0) | (1 << 1) | (1 << 2));
    hart.write_csr(SCOUNTEREN, (1 << 0) | (1 << 1) | (1 << 2));

    for _ in 0..64 {
        hart.step(&mut bus);
        if hart.shutdown_requested() {
            break;
        }
    }

    assert!(hart.shutdown_requested(), "SBI shutdown should be observed");
    assert_eq!(hart.read_reg(11), DTB_ADDR, "a1 should keep DTB pointer");
}
