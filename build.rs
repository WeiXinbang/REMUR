fn main() {
    // C 参考模型（difftest 用）——暂时跳过，M6 再启用
    // cc::Build::new()
    //     .file("c_src/ref_wrapper.c")
    //     .include("c_src")
    //     .compile("riscv_ref");
    // println!("cargo:rerun-if-changed=c_src/ref_wrapper.c");
    // println!("cargo:rerun-if-changed=c_src/mini-rv32ima.h");
}
