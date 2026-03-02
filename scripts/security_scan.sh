#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "[scan] rust dependency scan"
if command -v cargo-audit >/dev/null 2>&1; then
  (cd "$ROOT_DIR" && cargo audit)
else
  echo "[scan] cargo-audit not found; skipping local rust audit (CI job enforces this)"
fi

echo "[scan] dashboard npm audit (high+)"
(cd "$ROOT_DIR/dashboard" && npm audit --audit-level=high)

echo "[scan] container image scan"
if command -v docker >/dev/null 2>&1 && command -v trivy >/dev/null 2>&1; then
  docker build -t sync-server:phase6 "$ROOT_DIR"
  docker build -t sync-dashboard:phase6 "$ROOT_DIR/dashboard"
  trivy image --severity HIGH,CRITICAL --exit-code 1 sync-server:phase6
  trivy image --severity HIGH,CRITICAL --exit-code 1 sync-dashboard:phase6
else
  echo "[scan] docker or trivy missing; skipping local container scan (CI job enforces this)"
fi

echo "[scan] complete"
