mod common;

macro_rules! riscv_tests {
    ($prefix:expr, $($name:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                common::run_test(&format!("{}-{}", $prefix, stringify!($name)));
            }
        )+
    };
}

// rv32si 测试：5/6 通过
// dirty — 需要 Sv32 页表（Step 3.10）
riscv_tests! { "rv32si-p",
    csr, ma_fetch, sbreak, scall, wfi,
}
