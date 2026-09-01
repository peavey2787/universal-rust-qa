$ErrorActionPreference = "Stop"

# Keep the project itself on its pinned MSRV, but install independently-versioned
# Cargo QA utilities with current stable Rust so a newer cargo-deny/etc. does not
# accidentally force the workspace MSRV forward.
& rustup toolchain install stable --profile minimal
if ($LASTEXITCODE -ne 0) { throw "stable toolchain install failed" }
& rustup component add --toolchain 1.85.0 rustfmt clippy llvm-tools-preview
if ($LASTEXITCODE -ne 0) { throw "workspace component install failed" }
& rustup toolchain install nightly --profile minimal --component rust-src
if ($LASTEXITCODE -ne 0) { throw "nightly toolchain install failed" }

# Discover Cargo plugins by their `cargo-<name>[.exe]` executable first so a missing
# plugin never turns `cargo <subcommand> --version` into a bootstrap error. Once the
# executable exists, probe it through Cargo's real plugin invocation contract.
function Install-CargoTool([string]$Executable, [string]$Crate) {
  if (Get-Command $Executable -ErrorAction SilentlyContinue) {
    $Subcommand = $Executable.Substring("cargo-".Length)
    try {
      & cargo +stable $Subcommand --version *> $null
      if ($LASTEXITCODE -eq 0) {
        Write-Host "Found $Executable."
        return
      }
    } catch {
      # Fall through and reinstall a present-but-broken Cargo subcommand.
    }
    Write-Warning "$Executable exists but 'cargo $Subcommand --version' failed; reinstalling $Crate."
  }

  Write-Host "Installing $Crate with current stable Rust..."
  & cargo +stable install --locked $Crate
  if ($LASTEXITCODE -ne 0) {
    Write-Warning "Could not install $Crate; the corresponding QA gate will report unavailable."
    return
  }

  # Refresh command discovery after cargo install and fail visibly if installation
  # reported success but did not expose the expected Cargo plugin executable.
  $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
  if (-not (Get-Command $Executable -ErrorAction SilentlyContinue)) {
    Write-Warning "$Crate installation completed but $Executable is not discoverable on PATH."
  }
}

Install-CargoTool "cargo-llvm-cov" "cargo-llvm-cov"
Install-CargoTool "cargo-mutants" "cargo-mutants"
Install-CargoTool "cargo-fuzz" "cargo-fuzz"
Install-CargoTool "cargo-deny" "cargo-deny"
Install-CargoTool "cargo-hack" "cargo-hack"
Install-CargoTool "cargo-machete" "cargo-machete"
Install-CargoTool "cargo-semver-checks" "cargo-semver-checks"
Install-CargoTool "cargo-bloat" "cargo-bloat"
Install-CargoTool "cargo-llvm-lines" "cargo-llvm-lines"
Install-CargoTool "cargo-asm" "cargo-asm"
Install-CargoTool "cargo-insta" "cargo-insta"

function Find-WindowsAsanRuntime([string]$InstallPath) {
  if ([string]::IsNullOrWhiteSpace($InstallPath)) { return $null }

  $ToolsRoot = Join-Path $InstallPath "VC\Tools\MSVC"
  if (-not (Test-Path $ToolsRoot)) { return $null }

  $Versions = @(Get-ChildItem -Path $ToolsRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending)
  foreach ($Version in $Versions) {
    foreach ($Relative in @("bin\Hostx64\x64", "bin\Hostx86\x64")) {
      $RuntimeDir = Join-Path $Version.FullName $Relative
      $RuntimeDll = Join-Path $RuntimeDir "clang_rt.asan_dynamic-x86_64.dll"
      if (Test-Path $RuntimeDll) { return $RuntimeDir }
    }
  }
  return $null
}

function Export-WindowsAsanRuntime([string]$InstallPath) {
  $RuntimeDir = Find-WindowsAsanRuntime $InstallPath
  if ([string]::IsNullOrWhiteSpace($RuntimeDir)) {
    Write-Warning "The Visual Studio ASan component is installed, but clang_rt.asan_dynamic-x86_64.dll was not found beneath $InstallPath."
    return $false
  }

  $env:QA_ASAN_RUNTIME_DIR = $RuntimeDir
  $PathEntries = @($env:Path -split ';')
  if ($PathEntries -notcontains $RuntimeDir) {
    $env:Path = "$RuntimeDir;$env:Path"
  }
  Write-Host "Windows ASan runtime: $RuntimeDir"
  return $true
}

function Ensure-WindowsAsanComponent {
  $VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  $Setup = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\setup.exe"
  if (-not (Test-Path $VsWhere) -or -not (Test-Path $Setup)) {
    Write-Warning "Visual Studio Installer/vswhere not found; Windows ASan may be unavailable."
    return
  }

  $AsanInstall = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.ASAN -property installationPath
  if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($AsanInstall)) {
    $VcInstall = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($VcInstall)) {
      Write-Warning "MSVC C++ build tools were not found; Windows ASan cannot be provisioned automatically."
      return
    }

    Write-Host "Installing the Visual Studio C++ AddressSanitizer component..."
    & $Setup modify --installPath $VcInstall --add Microsoft.VisualStudio.Component.VC.ASAN --passive --norestart
    if ($LASTEXITCODE -ne 0) {
      Write-Warning "Visual Studio ASan component installation failed; the sanitizer gate will report the linker/runtime diagnostic."
      return
    }

    $AsanInstall = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.ASAN -property installationPath
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($AsanInstall)) {
      Write-Warning "Visual Studio installer returned success, but the C++ AddressSanitizer component is still not discoverable."
      return
    }
  } else {
    Write-Host "Found Microsoft.VisualStudio.Component.VC.ASAN."
  }

  [void](Export-WindowsAsanRuntime $AsanInstall)
}
Ensure-WindowsAsanComponent
