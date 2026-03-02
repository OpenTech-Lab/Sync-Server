#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-backup}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$ROOT_DIR/.drills/$STAMP"

DATABASE_URL="${DATABASE_URL:-postgres://sync:${POSTGRES_PASSWORD:-devpassword}@localhost:5432/sync_dev}"
RESTORE_DB="sync_restore_drill"
CONTAINER_DUMP_FILE="/var/lib/postgresql/data/.drill.dump"

POSTGRES_CONTAINER_ID="$(docker compose -f "$ROOT_DIR/docker-compose.yml" ps -q postgres 2>/dev/null || true)"

ensure_container() {
  if [[ -z "$POSTGRES_CONTAINER_ID" ]]; then
    echo "[drill] postgres container not found. Start it with: docker compose up -d postgres"
    exit 1
  fi
}

dump_database() {
  local out_file="$1"
  if command -v pg_dump >/dev/null 2>&1; then
    pg_dump "$DATABASE_URL" --format=custom --file "$out_file"
    return
  fi

  ensure_container
  docker exec "$POSTGRES_CONTAINER_ID" pg_dump \
    -U sync \
    -d sync_dev \
    --format=custom \
    --file "$CONTAINER_DUMP_FILE"
  docker cp "$POSTGRES_CONTAINER_ID":"$CONTAINER_DUMP_FILE" "$out_file"
}

restore_database() {
  local dump_file="$1"
  if command -v pg_restore >/dev/null 2>&1 && command -v createdb >/dev/null 2>&1 && command -v dropdb >/dev/null 2>&1; then
    dropdb --if-exists "$RESTORE_DB"
    createdb "$RESTORE_DB"
    pg_restore \
      --no-owner \
      --no-privileges \
      --dbname "postgres://sync:${POSTGRES_PASSWORD:-devpassword}@localhost:5432/$RESTORE_DB" \
      "$dump_file"
    return
  fi

  ensure_container
  docker cp "$dump_file" "$POSTGRES_CONTAINER_ID":"$CONTAINER_DUMP_FILE"
  docker exec "$POSTGRES_CONTAINER_ID" sh -lc "dropdb -U sync --if-exists $RESTORE_DB && createdb -U sync $RESTORE_DB"
  docker exec "$POSTGRES_CONTAINER_ID" pg_restore \
    -U sync \
    -d "$RESTORE_DB" \
    --no-owner \
    --no-privileges \
    "$CONTAINER_DUMP_FILE"
}

sanity_query() {
  local query="select 'users' as table_name, coalesce((select n_live_tup::bigint from pg_stat_user_tables where relname='users'), 0) as row_count union all select 'messages' as table_name, coalesce((select n_live_tup::bigint from pg_stat_user_tables where relname='messages'), 0) as row_count;"

  if command -v psql >/dev/null 2>&1; then
    psql "postgres://sync:${POSTGRES_PASSWORD:-devpassword}@localhost:5432/$RESTORE_DB" \
      -c "$query"
    return
  fi

  ensure_container
  docker exec "$POSTGRES_CONTAINER_ID" psql \
    -U sync \
    -d "$RESTORE_DB" \
    -c "$query"
}

mkdir -p "$OUT_DIR"

echo "[drill] output dir: $OUT_DIR"

echo "[drill] creating postgres dump"
dump_database "$OUT_DIR/database.dump"

echo "[drill] archiving critical config"
tar -czf "$OUT_DIR/configs.tar.gz" \
  -C "$ROOT_DIR" \
  .env.example docker-compose.yml api/openapi.yaml

if [[ "$MODE" == "restore" ]]; then
  echo "[drill] running restore validation into database: $RESTORE_DB"
  restore_database "$OUT_DIR/database.dump"

  echo "[drill] sanity query"
  sanity_query
fi

echo "[drill] complete"
