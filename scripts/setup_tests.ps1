# scripts/setup_tests.ps1

# 1. 自动转换当前路径为 WSL 路径
$WIN_PATH = Get-Location
$WSL_PATH = wsl wslpath "$WIN_PATH"
$ISA_DIR = "$WSL_PATH/riscv-tests/isa"
$BIN_OUT_DIR = "$WIN_PATH\tests\bins"

# 2. 确保 Windows 侧的输出目录存在
if (!(Test-Path $BIN_OUT_DIR)) {
    New-Item -ItemType Directory -Path $BIN_OUT_DIR -Force | Out-Null
}

Write-Host ">>> Starting WSL Build for RISC-V Tests (RV32)..." -ForegroundColor Cyan

# 3. 在 WSL 中执行编译
# XLEN=32 确保只编译 32 位版本的测试
wsl bash -c "cd $ISA_DIR && make -j XLEN=32"

if ($LASTEXITCODE -ne 0) {
    Write-Host "!!! Build failed in WSL. Please check if 'gcc-riscv64-unknown-elf' is installed." -ForegroundColor Red
    exit 1
}

Write-Host ">>> Converting ELF to Raw Binary and Importing..." -ForegroundColor Cyan

# 4. 定义我们要提取的测试类型
$TEST_TYPES = @("rv32ui-p-*", "rv32um-p-*", "rv32ua-p-*", "rv32mi-p-*", "rv32si-p-*")

foreach ($type in $TEST_TYPES) {
    $ELFS = Get-ChildItem -Path ".\riscv-tests\isa" -Filter $type | Where-Object { $_.Extension -eq "" }

    foreach ($elf in $ELFS) {
        $bin_name = $elf.Name + ".bin"
        $target_path = Join-Path $BIN_OUT_DIR $bin_name
        
        $elf_wsl_path = "$ISA_DIR/$($elf.Name)"
        # 转换反斜杠为正斜杠供 WSL 使用
        $bin_wsl_path = $WSL_PATH + "/tests/bins/" + $bin_name
        $bin_wsl_path = $bin_wsl_path.Replace('\', '/')
        
        wsl riscv64-unknown-elf-objcopy -O binary $elf_wsl_path $bin_wsl_path
        
        Write-Host "  [+] Imported: $bin_name" -ForegroundColor Gray
    }
}

Write-Host "`n>>> Success! All tests are ready in $BIN_OUT_DIR" -ForegroundColor Green
Write-Host ">>> You can now run 'cargo test' to start verification." -ForegroundColor Yellow
