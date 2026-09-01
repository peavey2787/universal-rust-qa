$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root
$LogDir = Join-Path $Root "qa-out\self-hardening"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$Log = Join-Path $LogDir ("windows-{0}.log" -f (Get-Date -Format "yyyyMMdd-HHmmss"))
Start-Transcript -Path $Log -Append | Out-Null
$ExitCode = 1
try {
  Write-Host ""
  Write-Host "Universal Rust QA - Windows full test + self-hardening"
  Write-Host "======================================================"

  # Bootstrap/tool prerequisites are fail-fast. Static/test gates are collected
  # together, but the expensive self-hardening campaign starts only when they pass.
  & (Join-Path $PSScriptRoot "bootstrap.ps1")
  $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
  if ($env:QA_SKIP_TOOL_INSTALL -ne "1") { & (Join-Path $PSScriptRoot "install-qa-tools.ps1") }
  if (-not (Test-Path (Join-Path $Root "Cargo.lock"))) {
    Write-Host "Generating local Cargo.lock for --locked release gates..."
    & cargo generate-lockfile
    if ($LASTEXITCODE -ne 0) { throw "cargo generate-lockfile failed" }
  }

  $Failures = New-Object System.Collections.Generic.List[string]
  function Invoke-Step([string]$Name,[scriptblock]$Command) {
    Write-Host ""
    Write-Host "==> $Name"
    & $Command
    $Code = $LASTEXITCODE
    if ($null -eq $Code) { $Code = 0 }
    if ($Code -eq 0) {
      Write-Host "PASS: $Name"
    } else {
      Write-Host "FAIL: $Name (exit $Code)"
      $script:Failures.Add($Name)
    }
  }

  Invoke-Step "cargo fmt --check" { cargo fmt --all -- --check }
  Invoke-Step "cargo check" { cargo check --workspace --all-targets --all-features --locked }
  Invoke-Step "cargo clippy -D warnings" { cargo clippy --workspace --all-targets --all-features --locked -- -D warnings }
  Invoke-Step "cargo test" { cargo test --workspace --all-targets --all-features --locked }
  Invoke-Step "cargo doctest" { cargo test --workspace --doc --locked }
  Invoke-Step "cargo qa doctor" { cargo run --locked -p cargo-qa -- qa doctor }
  if ($Failures.Count -eq 0) {
    Invoke-Step "cargo qa self-hardening" { cargo run --locked -p cargo-qa -- qa self-hardening }
  } else {
    Write-Host ""
    Write-Host "SKIP: cargo qa self-hardening because prerequisite gates failed."
  }

  # Diagnostic reruns do not alter the authoritative gate result. They run only
  # after a failed gate so the final transcript preserves the exact rustfmt,
  # Clippy, and libtest diagnostics instead of ending on a wrapper summary.
  if ($Failures.Contains("cargo fmt --check")) {
    Write-Host ""
    Write-Host "==> Diagnostic rerun: cargo fmt --check"
    & cargo fmt --all -- --check
  }
  if ($Failures.Contains("cargo clippy -D warnings")) {
    Write-Host ""
    Write-Host "==> Diagnostic rerun: cargo clippy -D warnings"
    & cargo clippy --workspace --all-targets --all-features --locked --message-format=short -- -D warnings
  }
  if ($Failures.Contains("cargo test")) {
    Write-Host ""
    Write-Host "==> Diagnostic rerun: qa-rules unit tests"
    & cargo test --locked -p qa-rules --lib --all-features -- --nocapture
  }

  Write-Host ""
  Write-Host "======================================================"
  if ($Failures.Count -eq 0) {
    Write-Host "PASS: all Windows tests and self-hardening completed."
    $ExitCode = 0
  } else {
    Write-Host ("FAIL: {0} top-level step(s) failed:" -f $Failures.Count)
    foreach ($Failure in $Failures) { Write-Host "  - $Failure" }
    $ExitCode = 1
  }
  Write-Host "Transcript: $Log"
  Write-Host "Reports: $Root\qa-out"
} catch {
  Write-Host ""
  Write-Error $_
  Write-Host "Full transcript: $Log"
  $ExitCode = 1
} finally {
  try { Stop-Transcript | Out-Null } catch { }
}
exit $ExitCode
