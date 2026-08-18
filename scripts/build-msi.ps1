param(
  [switch]$NoPause
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Test-Command {
  param([Parameter(Mandatory = $true)][string]$Name)
  return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Find-VcVars64Path {
  $candidates = @(
    "$env:ProgramFiles\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
    "$env:ProgramFiles\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat",
    "$env:ProgramFiles\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat",
    "$env:ProgramFiles\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat",
    "$env:ProgramFiles(x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
    "$env:ProgramFiles(x86)\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvars64.bat",
    "$env:ProgramFiles(x86)\Microsoft Visual Studio\2019\Professional\VC\Auxiliary\Build\vcvars64.bat",
    "$env:ProgramFiles(x86)\Microsoft Visual Studio\2019\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
  )

  foreach ($path in $candidates) {
    if (Test-Path -LiteralPath $path) {
      return $path
    }
  }

  return $null
}

function Import-BatchEnvironment {
  param([Parameter(Mandatory = $true)][string]$BatchFilePath)

  $cmdOutput = cmd.exe /d /s /c "`"$BatchFilePath`" >nul && set"
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to initialize MSVC environment via: $BatchFilePath"
  }

  foreach ($line in $cmdOutput) {
    $parts = $line -split "=", 2
    if ($parts.Count -eq 2) {
      [Environment]::SetEnvironmentVariable($parts[0], $parts[1], "Process")
    }
  }
}

function Run-Step {
  param(
    [Parameter(Mandatory = $true)][string]$Description,
    [Parameter(Mandatory = $true)][scriptblock]$Action
  )
  Write-Host ""
  Write-Host "[STEP] $Description"
  & $Action
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $repoRoot

Write-Host "============================================"
Write-Host " VibeShell Installer Build (Windows)"
Write-Host " Builds: desktop app + native vibeshell CLI sidecar + bundled Skill"
Write-Host " Output: NSIS (.exe) and MSI (.msi)"
Write-Host "============================================"

if (-not (Test-Command "node")) {
  throw "Node.js not found in PATH."
}

if (-not (Test-Command "npx")) {
  throw "npx not found in PATH."
}

if (-not (Test-Command "cargo")) {
  throw "Rust (cargo) not found in PATH."
}

if (-not (Test-Command "cl")) {
  $vcvarsPath = Find-VcVars64Path
  if ($null -eq $vcvarsPath) {
    throw "MSVC compiler not found. Install VS Build Tools (C++ workload)."
  }

  $originalPath = $env:Path
  Run-Step "Loading MSVC environment from vcvars64.bat..." {
    Import-BatchEnvironment -BatchFilePath $vcvarsPath
  }
  $env:Path = "$env:Path;$originalPath"
}

if (-not (Test-Command "cl")) {
  throw "MSVC compiler (cl.exe) is still unavailable after loading vcvars64.bat."
}

foreach ($name in @("CC", "CXX", "CI", "STATIC_VCRUNTIME")) {
  Remove-Item "Env:$name" -ErrorAction SilentlyContinue
}

# ── 0. Kill stale processes that may lock build outputs ───────────────
Run-Step "Killing stale VibeShell processes..." {
  $procs = @(
    Get-Process -Name "vibeshell" -ErrorAction SilentlyContinue
    Get-Process -Name "vibeshell-desktop" -ErrorAction SilentlyContinue
  )
  if ($procs.Count -gt 0) {
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue
    Write-Host "  Killed $($procs.Count) VibeShell process(es)"
  }
  Start-Sleep -Milliseconds 500
}

# ── 1. Build the target-suffixed native CLI sidecar ───────────────────
Run-Step "Building native vibeshell CLI sidecar..." {
  & node scripts/prepare-sidecar.mjs --target x86_64-pc-windows-msvc
  if ($LASTEXITCODE -ne 0) {
    throw "native CLI build failed with exit code $LASTEXITCODE."
  }
}

# ── 2. Build Tauri installers (NSIS + MSI) ────────────────────────────
Run-Step "Building NSIS and MSI bundles with Tauri..." {
  & npx tauri build --config src-tauri/tauri.sidecar.conf.json --bundles nsis --bundles msi
  if ($LASTEXITCODE -ne 0) {
    throw "tauri build failed with exit code $LASTEXITCODE."
  }
}

# ── 3. Report output ─────────────────────────────────────────────────
$bundleDirs = @(
  (Join-Path $repoRoot "target\release\bundle"),
  (Join-Path $repoRoot "src-tauri\target\release\bundle")
)

$bundleDir = $bundleDirs | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($null -eq $bundleDir) {
  throw "Build command finished, but bundle output directory was not found."
}

Write-Host ""
Write-Host "============================================"
Write-Host " Build Complete!"
Write-Host "============================================"

$nsisDir = Join-Path $bundleDir "nsis"
if (Test-Path $nsisDir) {
  $nsisFiles = @(Get-ChildItem -Path $nsisDir -Filter *.exe -File | Sort-Object LastWriteTime -Descending)
  if ($nsisFiles.Count -gt 0) {
    Write-Host "[OK] NSIS installer: $($nsisFiles[0].FullName)"
  }
}

$msiDir = Join-Path $bundleDir "msi"
if (Test-Path $msiDir) {
  $msiFiles = @(Get-ChildItem -Path $msiDir -Filter *.msi -File | Sort-Object LastWriteTime -Descending)
  if ($msiFiles.Count -gt 0) {
    Write-Host "[OK] MSI installer:  $($msiFiles[0].FullName)"
  }
}

Write-Host ""
Write-Host "Both installers include the GUI, native vibeshell command, and bundled Coding Agent Skill."
