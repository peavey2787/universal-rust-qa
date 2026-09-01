$ErrorActionPreference = "Stop"
if (Get-Command cargo -ErrorAction SilentlyContinue) { exit 0 }
Write-Host "Rust/Cargo not found. Installing the minimal stable rustup toolchain..."
$exe = Join-Path $env:TEMP "rustup-init.exe"
Invoke-WebRequest -UseBasicParsing -Uri "https://win.rustup.rs/x86_64" -OutFile $exe
& $exe -y --profile minimal
if ($LASTEXITCODE -ne 0) { throw "rustup-init failed with exit code $LASTEXITCODE" }
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
