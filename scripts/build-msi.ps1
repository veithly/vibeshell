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
Write-Host " Builds: vibeshell (GUI) + vshell (CLI)"
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
Run-Step "Killing stale vshell / vibeshell processes..." {
  foreach ($name in @("vshell", "vibeshell")) {
    $procs = @(Get-Process -Name $name -ErrorAction SilentlyContinue)
    if ($procs.Count -gt 0) {
      $procs | Stop-Process -Force -ErrorAction SilentlyContinue
      Write-Host "  Killed $($procs.Count) $name process(es)"
    }
  }
  Start-Sleep -Milliseconds 500
}

# ── 1. Detect Rust target triple ──────────────────────────────────────
Run-Step "Detecting Rust target triple..." {
  $rustcOutput = & rustc -vV 2>&1
  $hostLine = ($rustcOutput | Where-Object { $_ -match "^host:" }) -replace "host:\s*", ""
  $script:targetTriple = $hostLine.Trim()

  if ([string]::IsNullOrEmpty($script:targetTriple)) {
    throw "Could not detect Rust target triple from 'rustc -vV'"
  }

  Write-Host "  Target: $script:targetTriple"
}

# ── 2. Build vshell CLI ───────────────────────────────────────────────
Run-Step "Building vshell CLI (release)..." {
  & cargo build --release --package vshell
  if ($LASTEXITCODE -ne 0) {
    throw "vshell CLI build failed with exit code $LASTEXITCODE."
  }
}

# ── 3. Copy CLI binary to Tauri sidecar location ─────────────────────
Run-Step "Copying vshell CLI to sidecar location..." {
  $binDir = Join-Path $repoRoot "src-tauri\binaries"
  if (-not (Test-Path $binDir)) {
    New-Item -ItemType Directory -Path $binDir -Force | Out-Null
  }

  $src = Join-Path $repoRoot "target\release\vshell.exe"
  $dest = Join-Path $binDir "vshell-$script:targetTriple.exe"

  if (-not (Test-Path $src)) {
    throw "vshell CLI binary not found at: $src"
  }

  Copy-Item -Path $src -Destination $dest -Force
  Write-Host "  $src -> $dest"

  $fileSize = (Get-Item $dest).Length
  Write-Host "  Size: $([math]::Round($fileSize / 1MB, 2)) MB"
}

# ── 4. Build Tauri installers (NSIS + MSI) ────────────────────────────
Run-Step "Building NSIS and MSI bundles with Tauri..." {
  & npx tauri build --bundles nsis --bundles msi
  if ($LASTEXITCODE -ne 0) {
    throw "tauri build failed with exit code $LASTEXITCODE."
  }
}

# ── 5. Report output ─────────────────────────────────────────────────
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
Write-Host "Both installers include vibeshell (GUI) + vshell (CLI)."
Write-Host "vshell will be added to user PATH on installation."
