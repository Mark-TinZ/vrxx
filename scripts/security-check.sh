#!/usr/bin/env bash
# ==============================================================================
# security-check.sh
# Unified local security and code quality scanner for vrxx
# ==============================================================================

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "============================================================"
echo "🛡️  RUNNING COMPREHENSIVE SECURITY & QUALITY LINTER SUITE"
echo "============================================================"

FAILED=0

# 1. GITLEAKS (Secret Scanning)
echo ""
echo "▶ [1/4] Running Gitleaks (Secrets detection)..."
if command -v gitleaks &>/dev/null; then
    gitleaks detect --source . -v --redact --config .gitleaks.toml || { echo "❌ Gitleaks detected secrets!"; FAILED=1; }
elif docker info &>/dev/null; then
    echo "  (Using Docker for Gitleaks)"
    docker run --rm -v "$ROOT_DIR":/path zricethezav/gitleaks:latest detect --source="/path" -v --redact --config="/path/.gitleaks.toml" || { echo "❌ Gitleaks detected secrets!"; FAILED=1; }
else
    echo "⚠️  gitleaks not installed and docker not available. Install: https://github.com/gitleaks/gitleaks"
fi

# 2. SEMGREP (SAST: Secrets, OWASP)
echo ""
echo "▶ [2/4] Running Semgrep (SAST)..."
if command -v semgrep &>/dev/null; then
    semgrep scan \
        --config "p/secrets" \
        --config "p/owasp-top-ten" \
        --error \
        --metrics=off || { echo "❌ Semgrep reported security findings!"; FAILED=1; }
elif docker info &>/dev/null; then
    echo "  (Using Docker for Semgrep)"
    docker run --rm -v "$ROOT_DIR":/src returntocorp/semgrep semgrep scan \
        --config "p/secrets" \
        --config "p/owasp-top-ten" \
        --error \
        --metrics=off || { echo "❌ Semgrep reported security findings!"; FAILED=1; }
else
    echo "⚠️  semgrep not installed. Install: pip install semgrep"
fi

# 3. CARGO AUDIT (RustSec Vulnerability Database)
echo ""
echo "▶ [3/4] Running cargo-audit (RustSec CVEs)..."
CARGO_AUDIT_BIN=""
if command -v cargo-audit &>/dev/null; then
    CARGO_AUDIT_BIN="cargo audit"
elif command -v cargo &>/dev/null && cargo audit --version &>/dev/null; then
    CARGO_AUDIT_BIN="cargo audit"
elif [ -x "/tmp/cargo-audit-x86_64-unknown-linux-musl-v0.22.2/cargo-audit" ]; then
    CARGO_AUDIT_BIN="/tmp/cargo-audit-x86_64-unknown-linux-musl-v0.22.2/cargo-audit audit"
elif docker info &>/dev/null; then
    echo "  (Using Docker for cargo-audit)"
fi

if [ -n "$CARGO_AUDIT_BIN" ]; then
    $CARGO_AUDIT_BIN || { echo "❌ cargo-audit detected vulnerable dependencies!"; FAILED=1; }
else
    echo "⚠️  cargo-audit not installed. Download binary or run: cargo install cargo-audit"
fi

# 4. CLIPPY & COMPILER LINTS
echo ""
echo "▶ [4/4] Running Cargo Clippy (Rust lints & security idioms)..."
if command -v cargo &>/dev/null; then
    cargo clippy -- -D warnings || { echo "❌ Cargo Clippy reported warnings/errors!"; FAILED=1; }
else
    echo "⚠️  cargo not installed."
fi

echo ""
echo "============================================================"
if [ $FAILED -eq 0 ]; then
    echo "🎉 ALL SECURITY & QUALITY CHECKS PASSED!"
else
    echo "❌ SOME CHECKS FAILED! Please review the output above."
    exit 1
fi
echo "============================================================"
