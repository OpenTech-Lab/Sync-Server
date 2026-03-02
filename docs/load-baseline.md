# Load / Performance Baseline

Date: 2026-03-02

## Goal
Produce a repeatable baseline for API latency and throughput under concurrent synthetic traffic.

## Script
Use:

`server/scripts/load_baseline.sh`

## Coverage
- Endpoint: `/health` (baseline liveness path)
- Method: GET
- Concurrency: configurable (`-c`)
- Request count: configurable (`-n`)

## Usage
From `server/`:

`./scripts/load_baseline.sh -u http://localhost:8080 -n 500 -c 25`

## Output
- Total requests
- Concurrency
- Mean latency (ms)
- p95 latency (ms)

## Baseline targets (Phase 6)
- Mean latency <= 150 ms
- p95 latency <= 250 ms

## Notes
- This baseline is intentionally lightweight and CI-friendly.
- For deeper workload modeling (auth + messages + websocket churn), follow-up with k6 scenarios.
