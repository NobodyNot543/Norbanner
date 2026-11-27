# Builds the one-step installer: Norbanner_<version>_x64-setup.exe.
# Usage: scripts/build-installer.ps1   (from any folder)
#
# With TAURI_SIGNING_PRIVATE_KEY (and TAURI_SIGNING_PRIVATE_KEY_PASSWORD) set,
# it also writes the updater signature (<installer>.sig). Without them it
# looks for the key in %USERPROFILE%\.tauri\norbanner.key (no password);
# without either it builds an unsigned installer, fine for local testing.
#
# Uses the MSVC toolchain when Visual Studio's build tools are installed (as
# in CI) and the GNU one otherwise, with a MinGW from winget (WinLibs) on PATH.

# Not 'Stop': Windows PowerShell turns cargo's progress on stderr into errors;
# each native step checks $LASTEXITCODE instead.
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $vswhere) {
        # The host toolchain, so outputs stay in target/release (CI expects it).
        $triple = 'x86_64-pc-windows-msvc'
        $targetArgs = @()
        $release = 'release'
    } else {
        $triple = 'x86_64-pc-windows-gnu'
        $mingw = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages') -Directory -Filter 'BrechtSanders.WinLibs*' -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path $_.FullName 'mingw64\bin' } |
            Where-Object { Test-Path $_ } |
            Select-Object -First 1
        if (-not $mingw) { throw 'Neither MSVC build tools nor WinLibs MinGW were found.' }
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$mingw;$env:PATH"
        $env:RUSTUP_TOOLCHAIN = "stable-$triple"
        $targetArgs = @('--target', $triple)
        $release = "$triple/release"
    }

    $target = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
    cargo build --release -p norbanner @targetArgs
    if ($LASTEXITCODE) { throw 'cargo build norbanner failed' }

    # Tauri bundles sidecars from src-tauri/binaries/<name>-<target triple>.exe.
    $binaries = Join-Path $root 'apps/settings/src-tauri/binaries'
    New-Item -ItemType Directory -Force $binaries -ErrorAction Stop | Out-Null
    Copy-Item (Join-Path $target "$release/norbanner.exe") (Join-Path $binaries "norbanner-$triple.exe") -Force -ErrorAction Stop

    $localKey = Join-Path $env:USERPROFILE '.tauri\norbanner.key'
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and (Test-Path $localKey)) {
        $env:TAURI_SIGNING_PRIVATE_KEY = $localKey
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
    }

    $configs = @('--config', 'src-tauri/tauri.bundle.conf.json')
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
        Write-Warning 'TAURI_SIGNING_PRIVATE_KEY is not set: building without the updater signature.'
        $unsigned = Join-Path $env:TEMP 'norbanner-unsigned.conf.json'
        '{ "bundle": { "createUpdaterArtifacts": false } }' | Set-Content -Encoding ascii $unsigned
        $configs += @('--config', $unsigned)
    }

    Push-Location (Join-Path $root 'apps/settings')
    try {
        pnpm tauri build @targetArgs @configs
        if ($LASTEXITCODE) { throw 'tauri build failed' }
    } finally { Pop-Location }

    Get-ChildItem (Join-Path $target "$release/bundle/nsis/*-setup.exe*") |
        Select-Object FullName, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 2) } }
} finally { Pop-Location }
