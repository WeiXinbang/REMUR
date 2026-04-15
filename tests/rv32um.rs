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

riscv_tests! { "rv32um-p",
    mul, mulh, mulhsu, mulhu,
    div, divu, rem, remu,
}
