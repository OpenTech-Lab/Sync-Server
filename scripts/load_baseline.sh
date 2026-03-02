#!/usr/bin/env bash
set -euo pipefail

BASE_URL="http://localhost:8080"
REQUESTS=200
CONCURRENCY=20

while getopts "u:n:c:" opt; do
  case $opt in
    u) BASE_URL="$OPTARG" ;;
    n) REQUESTS="$OPTARG" ;;
    c) CONCURRENCY="$OPTARG" ;;
    *) echo "usage: $0 [-u base_url] [-n requests] [-c concurrency]"; exit 1 ;;
  esac
done

TARGET="$BASE_URL/health"
TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT

echo "[baseline] target=$TARGET requests=$REQUESTS concurrency=$CONCURRENCY"

seq "$REQUESTS" | xargs -I{} -P "$CONCURRENCY" bash -lc '
  start=$(date +%s%3N)
  curl -sS -o /dev/null "$0" || true
  end=$(date +%s%3N)
  echo $((end-start))
' "$TARGET" >> "$TMP_FILE"

TOTAL=$(wc -l < "$TMP_FILE" | tr -d ' ')
MEAN=$(awk '{sum+=$1} END {if (NR==0) print 0; else printf "%.2f", sum/NR}' "$TMP_FILE")
P95=$(sort -n "$TMP_FILE" | awk -v n="$TOTAL" 'NR==int((95*n+99)/100){print $1}')

echo "[baseline] total_requests=$TOTAL"
echo "[baseline] mean_ms=$MEAN"
echo "[baseline] p95_ms=${P95:-0}"
