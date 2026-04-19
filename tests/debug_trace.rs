use remur::bus::{Bus, RAM_BASE};
use remur::cpu::{CAUSE_ILLEGAL_INST, Hart, MSTATUS, MSTATUS_MIE, MTVEC};
use remur::instruction::{IOp, Instruction};
use remur::memory::Memory;

const MEM_SIZE: usize = 128 * 1024 * 1024;

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xFFF;
    (imm << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

#[test]
fn step_snapshot_records_basic_instruction() {
    let mut bus = Bus::new(Memory::new(MEM_SIZE));
    let mut hart = Hart::new();
    let inst = addi(1, 0, 1);
    bus.ram.write32(0, inst);
    hart.pc = RAM_BASE;

    let snap = hart.step_snapshot(&mut bus);
    assert_eq!(snap.pc, RAM_BASE);
    assert_eq!(snap.raw_inst, Some(inst));
    assert_eq!(snap.next_pc, RAM_BASE + 4);
    assert_eq!(hart.read_reg(1), 1);
    assert!(snap.trap.is_none());
    match snap.decoded_inst {
        Some(Instruction::I {
            op: IOp::Addi,
            rd,
            rs1,
            imm,
        }) => {
            assert_eq!(rd, 1);
            assert_eq!(rs1, 0);
            assert_eq!(imm, 1);
        }
        other => panic!("unexpected decode result: {other:?}"),
    }
}

#[test]
fn step_snapshot_records_illegal_instruction_trap() {
    let mut bus = Bus::new(Memory::new(MEM_SIZE));
    let mut hart = Hart::new();
    let illegal = 0xffff_ffff;
    bus.ram.write32(0, illegal);
    hart.pc = RAM_BASE;
    hart.write_csr(MTVEC, RAM_BASE + 0x100);
    hart.write_csr(MSTATUS, MSTATUS_MIE);

    let snap = hart.step_snapshot(&mut bus);
    assert_eq!(snap.raw_inst, Some(illegal));
    assert_eq!(snap.next_pc, RAM_BASE + 0x100);
    let trap = snap.trap.expect("expected trap snapshot");
    assert_eq!(trap.cause, CAUSE_ILLEGAL_INST);
    assert_eq!(trap.trap_vector, RAM_BASE + 0x100);
}
