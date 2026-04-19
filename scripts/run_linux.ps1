param(
    [string]$Kernel,
    [string]$Dtb,
    [string]$Initramfs,
    [string]$KernelAddr = "0x80400000",
    [string]$DtbAddr = "0x82000000",
    [string]$InitramfsAddr = "0x83000000",
    [string]$Bootargs = "earlycon=uart8250,mmio,0x10000000 console=ttyS0",
    [string]$Cycles = "50000000"
)

$args = @(
    "run", "linux",
    "--kernel-addr", $KernelAddr,
    "--dtb-addr", $DtbAddr,
    "--initramfs-addr", $InitramfsAddr,
    "--bootargs", $Bootargs,
    "--cycles", $Cycles
)

if ($Kernel) {
    $args += @("--kernel", $Kernel)
}

if ($Dtb) {
    $args += @("--dtb", $Dtb)
}

if ($Initramfs) {
    $args += @("--initramfs", $Initramfs)
}

& cargo @args
exit $LASTEXITCODE
