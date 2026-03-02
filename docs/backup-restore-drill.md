# Backup & Restore Drill (PostgreSQL + Critical Config)

Date: 2026-03-02

## Goal
Validate that database and critical configuration can be backed up and restored within operational windows.

## Drill Script
Use:

`server/scripts/backup_restore_drill.sh`

## What it does
1. Creates timestamped output folder under `server/.drills/<timestamp>/`
2. Exports PostgreSQL dump via `pg_dump`
3. Archives critical config files (`.env.example`, `docker-compose.yml`, `api/openapi.yaml`)
4. Optionally restores dump into an isolated restore database (`sync_restore_drill`)
5. Runs sanity query to verify restored row counts in key tables

## Usage
From `server/`:

- Backup only:
  - `./scripts/backup_restore_drill.sh backup`
- Backup + restore validation:
  - `./scripts/backup_restore_drill.sh restore`

## Required environment
- `DATABASE_URL`
- `POSTGRES_PASSWORD` (if URL depends on it)
- Local PostgreSQL tools (`pg_dump`, `psql`, `createdb`, `dropdb`)

## Pass criteria
- Backup artifacts created with non-zero size
- Restore database created and schema load succeeds
- Sanity query returns expected non-negative counts for `users`, `messages`

## Failure handling
- Preserve drill folder for forensic inspection
- Capture script output in incident channel
- Re-run with explicit DB URL and increased verbosity
