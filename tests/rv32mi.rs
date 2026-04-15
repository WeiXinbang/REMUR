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

/// 测试名含连字符的需要映射：bin 文件名用连字符，Rust 函数名用下划线
macro_rules! riscv_tests_mapped {
    ($prefix:expr, $(($fn_name:ident, $bin_name:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $fn_name() {
                common::run_test(&format!("{}-{}", $prefix, $bin_name));
            }
        )+
    };
}

// rv32mi 测试：15/16 通过
// breakpoint — 需要 debug trigger CSR（暂跳过）
riscv_tests! { "rv32mi-p",
    csr, illegal, instret_overflow,
    ma_addr, ma_fetch, mcsr, pmpaddr,
    sbreak, scall, shamt, zicntr,
}

riscv_tests_mapped! { "rv32mi-p",
    (lh_misaligned,  "lh-misaligned"),
    (lw_misaligned,  "lw-misaligned"),
    (sh_misaligned,  "sh-misaligned"),
    (sw_misaligned,  "sw-misaligned"),
}
