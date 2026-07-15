$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Target = "x86_64-pc-windows-msvc"
$TauriManifest = Join-Path $ProjectRoot "src-tauri\Cargo.toml"
$SidecarDirectory = Join-Path $ProjectRoot "src-tauri\binaries"
$SidecarPlaceholder = Join-Path $SidecarDirectory "codex-turn-chime-hook-$Target.exe"

function Write-Step([string] $Message) {
  Write-Host "[build_windows_x64] $Message"
}

function Invoke-Checked([scriptblock] $Command, [string] $Description) {
  & $Command
  if ($LASTEXITCODE -ne 0) {
    throw "$Description failed with exit code $LASTEXITCODE."
  }
}

if ($env:OS -ne "Windows_NT") {
  throw "This script must run on Windows. Use scripts/build_macos_arm.sh on Apple Silicon macOS."
}

foreach ($CommandName in @("node", "npm", "rustc", "cargo", "rustup")) {
  if (-not (Get-Command $CommandName -ErrorAction SilentlyContinue)) {
    throw "$CommandName is required. Install Node.js 22, stable Rust MSVC, and the Tauri 2 Windows prerequisites."
  }
}

$NodeMajor = [int]((& node -p "process.versions.node.split('.')[0]").Trim())
if ($NodeMajor -ne 22) {
  throw "Node.js 22 is required; found $(& node --version)."
}

Set-Location $ProjectRoot
Write-Step "Using Node.js $(& node --version) and $(& rustc --version)"
Invoke-Checked { rustup target add $Target } "Adding Rust target $Target"
Write-Step "Installing locked npm dependencies..."
Invoke-Checked { npm ci } "npm ci"
Write-Step "Building frontend assets..."
Invoke-Checked { npm run build } "Frontend build"
Write-Step "Staging the sidecar placeholder required by the Tauri build script..."
New-Item -ItemType Directory -Path $SidecarDirectory -Force | Out-Null
[System.IO.File]::WriteAllBytes($SidecarPlaceholder, [byte[]]@())
Write-Step "Building the release Hook sidecar for $Target..."
Invoke-Checked { cargo build --manifest-path $TauriManifest --release --bin codex-turn-chime-hook --target $Target } "Hook sidecar build"
Write-Step "Staging the release Hook sidecar..."
Invoke-Checked { node (Join-Path $ProjectRoot "scripts\stage-sidecar.mjs") $Target release } "Hook sidecar staging"
Write-Step "Building NSIS and MSI installers..."
Invoke-Checked { npm run tauri build -- --target $Target --bundles "nsis,msi" } "Windows installer build"

Write-Step "Build complete."
Write-Step "NSIS: $ProjectRoot\src-tauri\target\$Target\release\bundle\nsis\"
Write-Step "MSI:  $ProjectRoot\src-tauri\target\$Target\release\bundle\msi\"
