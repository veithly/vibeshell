# VibeShell Test Runner (PowerShell)
# Usage: .\scripts\test.ps1 [-Backend] [-Frontend] [-All]
param(
    [switch]$Backend,
    [switch]$Frontend,
    [switch]$Integration,
    [switch]$All
)

$ErrorActionPreference = "Continue"
$Errors = 0

function Write-Info($msg) { Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Write-Pass($msg) { Write-Host "[PASS] $msg" -ForegroundColor Green }
function Write-Warn($msg) { Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Write-Fail($msg) { Write-Host "[FAIL] $msg" -ForegroundColor Red; $script:Errors++ }

# ============================================
# Backend Tests (Rust)
# ============================================
function Test-Backend {
    Write-Info "=== Backend Tests (Rust) ==="

    Write-Info "Running cargo check..."
    cargo check --manifest-path src-tauri/Cargo.toml 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Pass "cargo check passed" } else { Write-Fail "cargo check failed" }

    Write-Info "Running cargo test..."
    cargo test --manifest-path src-tauri/Cargo.toml 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Pass "cargo test passed" } else { Write-Fail "cargo test failed" }

    Write-Info "Running clippy..."
    cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Pass "clippy passed" } else { Write-Warn "clippy has warnings" }
}

# ============================================
# Frontend Tests (TypeScript)
# ============================================
function Test-Frontend {
    Write-Info "=== Frontend Tests (TypeScript) ==="

    Write-Info "Running TypeScript type check..."
    npx tsc --noEmit 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Pass "TypeScript check passed" } else { Write-Fail "TypeScript check failed" }

    Write-Info "Running Vite build..."
    npm run build 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Pass "Frontend build passed" } else { Write-Fail "Frontend build failed" }
}

# ============================================
# Integration Tests
# ============================================
function Test-Integration {
    Write-Info "=== Integration Tests ==="

    Write-Info "Checking i18n locale files..."
    $enKeys = node -e "const en=require('./src/i18n/locales/en.json');console.log(JSON.stringify(Object.keys(en).sort()))"
    $zhKeys = node -e "const zh=require('./src/i18n/locales/zh.json');console.log(JSON.stringify(Object.keys(zh).sort()))"
    if ($enKeys -eq $zhKeys) { Write-Pass "i18n locale keys match (en <-> zh)" } else { Write-Fail "i18n locale keys mismatch" }

    Write-Info "Checking Tauri config..."
    node -e "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8'))" 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Pass "tauri.conf.json is valid" } else { Write-Fail "tauri.conf.json is invalid" }

    Write-Info "Checking package.json..."
    node -e "JSON.parse(require('fs').readFileSync('package.json','utf8'))" 2>&1
    if ($LASTEXITCODE -eq 0) { Write-Pass "package.json is valid" } else { Write-Fail "package.json is invalid" }
}

# ============================================
# Main
# ============================================
if (-not ($Backend -or $Frontend -or $Integration -or $All)) { $All = $true }

if ($Backend -or $All) { Test-Backend; Write-Host "" }
if ($Frontend -or $All) { Test-Frontend; Write-Host "" }
if ($Integration -or $All) { Test-Integration; Write-Host "" }

Write-Host ""
if ($Errors -gt 0) {
    Write-Fail "Tests completed with $Errors error(s)"
    exit 1
} else {
    Write-Pass "All tests passed!"
}
