#!/bin/bash
# VibeShell Test Runner
# Usage: ./scripts/test.sh [--backend] [--frontend] [--all]
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info() { echo -e "${CYAN}[INFO]${NC} $1"; }
ok() { echo -e "${GREEN}[PASS]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; }

ERRORS=0

# ============================================
# Backend Tests (Rust)
# ============================================
run_backend_tests() {
  info "=== Backend Tests (Rust) ==="

  info "Running cargo check..."
  if cargo check --manifest-path src-tauri/Cargo.toml 2>&1; then
    ok "cargo check passed"
  else
    fail "cargo check failed"
    ERRORS=$((ERRORS + 1))
  fi

  info "Running cargo test..."
  if cargo test --manifest-path src-tauri/Cargo.toml 2>&1; then
    ok "cargo test passed"
  else
    fail "cargo test failed"
    ERRORS=$((ERRORS + 1))
  fi

  info "Running clippy..."
  if cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings 2>&1; then
    ok "clippy passed"
  else
    warn "clippy has warnings (non-fatal)"
  fi
}

# ============================================
# Frontend Tests (TypeScript)
# ============================================
run_frontend_tests() {
  info "=== Frontend Tests (TypeScript) ==="

  info "Running TypeScript type check..."
  if npx tsc --noEmit 2>&1; then
    ok "TypeScript check passed"
  else
    fail "TypeScript check failed"
    ERRORS=$((ERRORS + 1))
  fi

  info "Running Vite build..."
  if npm run build 2>&1; then
    ok "Frontend build passed"
  else
    fail "Frontend build failed"
    ERRORS=$((ERRORS + 1))
  fi
}

# ============================================
# Integration Tests
# ============================================
run_integration_tests() {
  info "=== Integration Tests ==="

  info "Checking i18n locale files..."
  EN_KEYS=$(node -e "const en = require('./src/i18n/locales/en.json'); console.log(JSON.stringify(Object.keys(en).sort()))")
  ZH_KEYS=$(node -e "const zh = require('./src/i18n/locales/zh.json'); console.log(JSON.stringify(Object.keys(zh).sort()))")
  if [ "$EN_KEYS" = "$ZH_KEYS" ]; then
    ok "i18n locale keys match (en ↔ zh)"
  else
    fail "i18n locale keys mismatch between en.json and zh.json"
    ERRORS=$((ERRORS + 1))
  fi

  info "Checking Tauri config..."
  if node -e "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json', 'utf8'))" 2>&1; then
    ok "tauri.conf.json is valid JSON"
  else
    fail "tauri.conf.json is invalid"
    ERRORS=$((ERRORS + 1))
  fi

  info "Checking package.json..."
  if node -e "JSON.parse(require('fs').readFileSync('package.json', 'utf8'))" 2>&1; then
    ok "package.json is valid JSON"
  else
    fail "package.json is invalid"
    ERRORS=$((ERRORS + 1))
  fi
}

# ============================================
# Main
# ============================================
MODE="${1:---all}"

case "$MODE" in
  --backend)
    run_backend_tests
    ;;
  --frontend)
    run_frontend_tests
    ;;
  --integration)
    run_integration_tests
    ;;
  --all)
    run_backend_tests
    echo ""
    run_frontend_tests
    echo ""
    run_integration_tests
    ;;
  *)
    echo "Usage: $0 [--backend] [--frontend] [--integration] [--all]"
    exit 1
    ;;
esac

echo ""
if [ $ERRORS -gt 0 ]; then
  fail "Tests completed with $ERRORS error(s)"
  exit 1
else
  ok "All tests passed!"
fi
