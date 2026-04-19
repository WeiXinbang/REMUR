use remur::bus::{Bus, RAM_BASE};
use remur::cpu::{Hart, MCAUSE};
use remur::execute;
use remur::instruction::Instruction;
use remur::memory::Memory;

#[test]
fn sbi_legacy_putchar_is_handled_without_trap() {
    let mut bus = Bus::new(Memory::new(1024 * 1024));
    let mut hart = Hart::new();
    hart.pc = RAM_BASE;
    hart.privilege = 1;
    hart.enable_sbi(true);

    hart.write_reg(10, b'A' as u32); // a0 = char
    hart.write_reg(17, 0x01); // a7 = legacy console_putchar

    execute::execute(&mut hart, &mut bus, Instruction::Ecall);

    assert_eq!(hart.pc, RAM_BASE + 4);
    assert_eq!(hart.read_reg(10), 0);
    assert_eq!(hart.read_csr(MCAUSE), 0);
}

#[test]
fn sbi_legacy_set_timer_programs_clint() {
    let mut bus = Bus::new(Memory::new(1024 * 1024));
    let mut hart = Hart::new();
    hart.pc = RAM_BASE;
    hart.privilege = 1;
    hart.enable_sbi(true);

    hart.write_reg(10, 123); // a0 low
    hart.write_reg(11, 0); // a1 high
    hart.write_reg(17, 0x00); // a7 = legacy set_timer

    execute::execute(&mut hart, &mut bus, Instruction::Ecall);

    assert_eq!(hart.pc, RAM_BASE + 4);
    assert_eq!(hart.read_reg(10), 0);
    assert_eq!(bus.clint.mtimecmp, 123);
}

#[test]
fn sbi_base_probe_reports_time_extension() {
    let mut bus = Bus::new(Memory::new(1024 * 1024));
    let mut hart = Hart::new();
    hart.pc = RAM_BASE;
    hart.privilege = 1;
    hart.enable_sbi(true);

    hart.write_reg(10, 0x5449_4D45); // a0 = TIME eid
    hart.write_reg(16, 3); // a6 = fid probe_extension
    hart.write_reg(17, 0x10); // a7 = BASE eid

    execute::execute(&mut hart, &mut bus, Instruction::Ecall);

    assert_eq!(hart.pc, RAM_BASE + 4);
    assert_eq!(hart.read_reg(10), 0); // a0 = SBI_SUCCESS
    assert_eq!(hart.read_reg(11), 1); // a1 = supported
}

#[test]
fn sbi_base_probe_reports_dbcn_extension() {
    let mut bus = Bus::new(Memory::new(1024 * 1024));
    let mut hart = Hart::new();
    hart.pc = RAM_BASE;
    hart.privilege = 1;
    hart.enable_sbi(true);

    hart.write_reg(10, 0x4442_434E); // a0 = DBCN eid
    hart.write_reg(16, 3); // a6 = fid probe_extension
    hart.write_reg(17, 0x10); // a7 = BASE eid

    execute::execute(&mut hart, &mut bus, Instruction::Ecall);

    assert_eq!(hart.pc, RAM_BASE + 4);
    assert_eq!(hart.read_reg(10), 0); // a0 = SBI_SUCCESS
    assert_eq!(hart.read_reg(11), 1); // a1 = supported
}

#[test]
fn sbi_shutdown_sets_exit_flag() {
    let mut bus = Bus::new(Memory::new(1024 * 1024));
    let mut hart = Hart::new();
    hart.pc = RAM_BASE;
    hart.privilege = 1;
    hart.enable_sbi(true);

    hart.write_reg(17, 0x08); // a7 = legacy shutdown
    execute::execute(&mut hart, &mut bus, Instruction::Ecall);

    assert_eq!(hart.pc, RAM_BASE + 4);
    assert!(hart.shutdown_requested());
}
