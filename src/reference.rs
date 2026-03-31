use std::os::raw::{c_uchar, c_uint};

extern "C" {
    fn ref_init(mem_size: c_uint, start_pc: c_uint);
    fn ref_load_bin(addr: c_uint, data: *const c_uchar, len: c_uint);
    fn ref_step();
    fn ref_get_reg(idx: c_uint) -> c_uint;
    fn ref_get_pc() -> c_uint;
}

pub struct ReferenceModel;

impl ReferenceModel {
    pub fn new(mem_size: usize, start_pc: u32) -> Self {
        unsafe { ref_init(mem_size as c_uint, start_pc as c_uint); }
        Self
    }

    pub fn load_bin(&self, addr: u32, data: &[u8]) {
        unsafe {
            ref_load_bin(addr as c_uint, data.as_ptr(), data.len() as c_uint);
        }
    }

    pub fn step(&self) {
        unsafe { ref_step(); }
    }

    pub fn get_reg(&self, idx: usize) -> u32 {
        unsafe { ref_get_reg(idx as c_uint) as u32 }
    }

    pub fn get_pc(&self) -> u32 {
        unsafe { ref_get_pc() as u32 }
    }
}
