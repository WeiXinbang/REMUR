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

// rv32si 测试：6/6 全部通过
riscv_tests! { "rv32si-p",
    csr, dirty, ma_fetch, sbreak, scall, wfi,
}
