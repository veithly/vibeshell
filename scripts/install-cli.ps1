param(
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\VibeShell CLI")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Source = Join-Path $PSScriptRoot "vibeshell.exe"
$Destination = Join-Path $InstallDir "vibeshell.exe"
if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
  throw "VibeShell CLI binary not found next to this installer: $Source"
}

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
$Temporary = Join-Path $InstallDir (".vibeshell-install-{0}.exe" -f $PID)
Copy-Item -LiteralPath $Source -Destination $Temporary -Force
Move-Item -LiteralPath $Temporary -Destination $Destination -Force

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$entries = @($userPath -split ";" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
if (-not ($entries | Where-Object { $_.TrimEnd('\') -ieq $InstallDir.TrimEnd('\') })) {
  $updated = @($entries + $InstallDir) -join ";"
  [Environment]::SetEnvironmentVariable("Path", $updated, "User")
}
$env:Path = "$InstallDir;$env:Path"

# The first native invocation installs the bundled Skill into every detected
# coding-agent directory plus the universal ~/.agents/skills location.
& $Destination version | Out-Null
if ($LASTEXITCODE -ne 0) {
  throw "VibeShell CLI verification failed with exit code $LASTEXITCODE"
}

Write-Host "Installed native VibeShell CLI: $Destination"
Write-Host "Open a new terminal, then run: vibeshell import auto --dry-run"
