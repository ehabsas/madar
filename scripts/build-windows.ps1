$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo --version | Out-Host
cargo tauri --version 2>$null | Out-Host
if ($LASTEXITCODE -ne 0) {
    Write-Host "Installing Tauri CLI (first build only)..."
    cargo install tauri-cli --locked
    if ($LASTEXITCODE -ne 0) { throw "Could not install tauri-cli." }
}

Write-Host "Building Madar installers for Windows (MSI + NSIS)..."
cargo tauri build --bundles msi,nsis
if ($LASTEXITCODE -ne 0) { throw "Madar Windows build failed." }
Write-Host "Artifacts are in src-tauri\target\release\bundle\msi and src-tauri\target\release\bundle\nsis"
