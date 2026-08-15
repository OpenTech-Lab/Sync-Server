# Agent Admin Tokens Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a server admin issue a long-lived, revocable "agent token" from the dashboard so an external AI agent can authenticate against the `/api/admin/*` API with full admin power, without sharing a human login.

**Architecture:** A new `agent_tokens` Postgres table stores a SHA-256 hash of a random `agt_<64-hex>` secret (the raw secret is shown to the admin exactly once). The existing JWT-based `AdminUser` request extractor (`src/auth/middleware.rs`) gains a fallback: if the bearer string isn't a valid JWT but starts with `agt_`, it's looked up by hash and, if active, synthesizes admin `Claims` — every existing admin route keeps working unchanged. A new dashboard page under the existing `(admin)` route group lets a human admin create, list, and revoke these tokens.

**Tech Stack:** Rust / actix-web / diesel / PostgreSQL (`Server/`), Next.js 16 / React 19 / TypeScript (`Server/dashboard/`).

**Spec:** `Server/docs/superpowers/specs/2026-08-15-agent-admin-tokens-design.md`

## Global Constraints

- Agent tokens carry **full admin power** (same as a human admin JWT) — no reduced scope in this iteration.
- Only the SHA-256 hash of a token is ever persisted. The raw secret is returned exactly once, in the `POST` create response, and never again.
- Token format: `agt_` followed by 64 lowercase hex characters (32 random bytes).
- Reuse `generate_refresh_token()` and `hash_token()` from `src/auth/tokens.rs` — do not add new crypto code.
- `AuthUser` (non-admin) routes must NOT accept agent tokens — only `AdminUser` does.
- Follow existing file organization: all `/api/admin/*` handlers live in `src/routes/admin.rs` (do not create a new routes file); all admin dashboard proxy routes live under `dashboard/app/api/admin/*` (do not create a new top-level api directory).
- Run `cargo fmt` and `cargo clippy --all-targets -- -D warnings` before every commit that touches Rust files (this is the existing repo convention — check `Server/scripts/` or CI config only if a task's build step fails unexpectedly).

---

### Task 1: Migration — `agent_tokens` table

**Files:**
- Create: `Server/migrations/2026-08-15-000026_agent_tokens/up.sql`
- Create: `Server/migrations/2026-08-15-000026_agent_tokens/down.sql`
- Modify: `Server/src/schema.rs` (append new table + joinable + allow_tables_to_appear_in_same_query entries)

**Interfaces:**
- Produces: Postgres table `agent_tokens` with columns `id, name, token_hash, token_prefix, created_by, created_at, expires_at, last_used_at, revoked_at`, and the diesel `agent_tokens` schema module used by Task 2's model.

- [ ] **Step 1: Write the migration SQL**

`Server/migrations/2026-08-15-000026_agent_tokens/up.sql`:

```sql
CREATE TABLE agent_tokens (
    id            UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    name          TEXT        NOT NULL,
    token_hash    TEXT        NOT NULL UNIQUE,
    token_prefix  TEXT        NOT NULL,
    created_by    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at    TIMESTAMPTZ,
    last_used_at  TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ
);

CREATE INDEX idx_agent_tokens_created_by ON agent_tokens (created_by, created_at DESC);
```

`Server/migrations/2026-08-15-000026_agent_tokens/down.sql`:

```sql
DROP TABLE IF EXISTS agent_tokens;
```

- [ ] **Step 2: Apply the migration and regenerate the schema**

Run (from `Server/`): `diesel migration run`

Expected: prints `Running migration 2026-08-15-000026_agent_tokens`, and `src/schema.rs` is automatically rewritten to include an `agent_tokens` table block.

If `diesel migration run` cannot connect to a local database (no `DATABASE_URL` / no Postgres running), instead hand-edit `src/schema.rs`:

1. Insert this block immediately after the existing `call_records` table block (search for `call_records (id) {` and its closing `}`, insert right after):

```rust
diesel::table! {
    use diesel::sql_types::*;

    agent_tokens (id) {
        id -> Uuid,
        name -> Text,
        token_hash -> Text,
        token_prefix -> Text,
        created_by -> Uuid,
        created_at -> Timestamptz,
        expires_at -> Nullable<Timestamptz>,
        last_used_at -> Nullable<Timestamptz>,
        revoked_at -> Nullable<Timestamptz>,
    }
}
```

2. In the `diesel::joinable!(...)` block (near the bottom of the file, alongside `diesel::joinable!(refresh_tokens -> users (user_id));`), add:

```rust
diesel::joinable!(agent_tokens -> users (created_by));
```

3. In the `diesel::allow_tables_to_appear_in_same_query!(...)` list at the very end of the file, add `agent_tokens,` as a new entry (any position in the list is fine, e.g. right after `server_news,`).

- [ ] **Step 3: Verify the crate still compiles**

Run: `cargo check` (from `Server/`)
Expected: no errors. (The `agent_tokens` table isn't referenced by any Rust code yet, so this only validates the schema syntax itself.)

- [ ] **Step 4: Commit**

```bash
cd Server
git add migrations/2026-08-15-000026_agent_tokens src/schema.rs
git commit -m "feat(server): add agent_tokens table"
```

---

### Task 2: Model — `AgentToken`

**Files:**
- Create: `Server/src/models/agent_token.rs`
- Modify: `Server/src/models/mod.rs`

**Interfaces:**
- Consumes: `agent_tokens` diesel table from Task 1 (`crate::schema::agent_tokens`).
- Produces: `AgentToken` struct (fields: `id: Uuid, name: String, token_hash: String, token_prefix: String, created_by: Uuid, created_at: DateTime<Utc>, expires_at: Option<DateTime<Utc>>, last_used_at: Option<DateTime<Utc>>, revoked_at: Option<DateTime<Utc>>`) and `NewAgentToken` struct (fields: `id: Uuid, name: String, token_hash: String, token_prefix: String, created_by: Uuid, expires_at: Option<DateTime<Utc>>`), both re-exported from `crate::models`. Used by Task 3's service.

- [ ] **Step 1: Create the model file**

`Server/src/models/agent_token.rs`:

```rust
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::agent_tokens;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = agent_tokens)]
pub struct AgentToken {
    pub id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = agent_tokens)]
pub struct NewAgentToken {
    pub id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub created_by: Uuid,
    pub expires_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 2: Register the module**

In `Server/src/models/mod.rs`, add to the alphabetically-ordered `pub mod` list (after `admin` and before `call_record`):

```rust
pub mod agent_token;
```

And add a re-export line (after the `admin` re-export, before `call_record`'s):

```rust
#[allow(unused_imports)]
pub use agent_token::{AgentToken, NewAgentToken};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check` (from `Server/`)
Expected: no errors (both structs are unused so far, which is fine — no `#[allow(dead_code)]` needed since they're `pub` and re-exported).

- [ ] **Step 4: Commit**

```bash
cd Server
git add src/models/agent_token.rs src/models/mod.rs
git commit -m "feat(server): add AgentToken model"
```

---

### Task 3: Service — `agent_token_service`

**Files:**
- Create: `Server/src/services/agent_token_service.rs`
- Modify: `Server/src/services/mod.rs`

**Interfaces:**
- Consumes: `crate::db::Pool`, `crate::errors::AppError`, `crate::models::{AgentToken, NewAgentToken}`, `crate::auth::tokens::{generate_refresh_token, hash_token}`.
- Produces:
  - `pub fn compute_expires_at(now: DateTime<Utc>, expires_in_days: Option<i64>) -> Result<Option<DateTime<Utc>>, AppError>`
  - `pub fn create(pool: &Pool, created_by: Uuid, name: &str, expires_in_days: Option<i64>) -> Result<(AgentToken, String), AppError>`
  - `pub fn list(pool: &Pool) -> Result<Vec<AgentToken>, AppError>`
  - `pub fn revoke(pool: &Pool, id: Uuid) -> Result<(), AppError>`
  - `pub fn verify(pool: &Pool, raw_token: &str) -> Result<Option<AgentToken>, AppError>`

  These four (plus `compute_expires_at`) are consumed by Task 4 (`verify`) and Task 5 (`create`, `list`, `revoke`).

- [ ] **Step 1: Write the failing unit tests for the pure logic**

`Server/src/services/agent_token_service.rs` (create the file with just this test module first):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 12, 0, 0).unwrap()
    }

    #[test]
    fn compute_expires_at_none_means_never_expires() {
        let result = compute_expires_at(fixed_now(), None).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn compute_expires_at_adds_days() {
        let result = compute_expires_at(fixed_now(), Some(30)).unwrap().unwrap();
        assert_eq!(result, fixed_now() + chrono::Duration::days(30));
    }

    #[test]
    fn compute_expires_at_rejects_non_positive_days() {
        assert!(compute_expires_at(fixed_now(), Some(0)).is_err());
        assert!(compute_expires_at(fixed_now(), Some(-5)).is_err());
    }

    #[test]
    fn normalize_name_trims_and_rejects_empty() {
        assert_eq!(normalize_name("  Claude ops agent  ").unwrap(), "Claude ops agent");
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name(&"a".repeat(121)).is_err());
    }

    #[test]
    fn token_prefix_is_stable_and_short() {
        let prefix = token_prefix("agt_abcdef0123456789");
        assert_eq!(prefix, "agt_abcdef01");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail (module doesn't exist yet)**

Run: `cargo test -p sync-server agent_token_service` (from `Server/`)
Expected: compile error — `compute_expires_at`, `normalize_name`, `token_prefix` not found (they don't exist yet), and the module isn't registered.

- [ ] **Step 3: Implement the service**

Prepend this above the `#[cfg(test)]` block in `Server/src/services/agent_token_service.rs`:

```rust
use chrono::{DateTime, Duration, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use crate::auth::tokens::{generate_refresh_token, hash_token};
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::{AgentToken, NewAgentToken};
use crate::schema::agent_tokens::dsl as tokens_dsl;

const MAX_NAME_CHARS: usize = 120;
const TOKEN_PREFIX_RAW_CHARS: usize = 8;

fn normalize_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("name cannot be empty".into()));
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "name must be <= {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn token_prefix(raw_token: &str) -> String {
    let without_prefix = raw_token.strip_prefix("agt_").unwrap_or(raw_token);
    let visible: String = without_prefix.chars().take(TOKEN_PREFIX_RAW_CHARS).collect();
    format!("agt_{visible}")
}

pub fn compute_expires_at(
    now: DateTime<Utc>,
    expires_in_days: Option<i64>,
) -> Result<Option<DateTime<Utc>>, AppError> {
    match expires_in_days {
        None => Ok(None),
        Some(days) if days <= 0 => Err(AppError::BadRequest(
            "expires_in_days must be a positive number of days".into(),
        )),
        Some(days) => Ok(Some(now + Duration::days(days))),
    }
}

/// Generates a new agent token, stores its hash, and returns the row plus
/// the raw secret. The raw secret is not recoverable after this call returns.
pub fn create(
    pool: &Pool,
    created_by: Uuid,
    name: &str,
    expires_in_days: Option<i64>,
) -> Result<(AgentToken, String), AppError> {
    let normalized_name = normalize_name(name)?;
    let expires_at = compute_expires_at(Utc::now(), expires_in_days)?;

    let (raw_suffix, hash) = generate_refresh_token();
    let raw_token = format!("agt_{raw_suffix}");

    let mut conn = pool.get()?;
    let payload = NewAgentToken {
        id: Uuid::new_v4(),
        name: normalized_name,
        token_hash: hash,
        token_prefix: token_prefix(&raw_token),
        created_by,
        expires_at,
    };

    diesel::insert_into(tokens_dsl::agent_tokens)
        .values(&payload)
        .execute(&mut conn)?;

    let row = tokens_dsl::agent_tokens
        .find(payload.id)
        .select(AgentToken::as_select())
        .first::<AgentToken>(&mut conn)
        .map_err(AppError::from)?;

    Ok((row, raw_token))
}

pub fn list(pool: &Pool) -> Result<Vec<AgentToken>, AppError> {
    let mut conn = pool.get()?;
    tokens_dsl::agent_tokens
        .order(tokens_dsl::created_at.desc())
        .select(AgentToken::as_select())
        .load::<AgentToken>(&mut conn)
        .map_err(AppError::from)
}

/// Idempotent: revoking an already-revoked (or nonexistent) token is not an error.
pub fn revoke(pool: &Pool, id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::update(tokens_dsl::agent_tokens.find(id))
        .filter(tokens_dsl::revoked_at.is_null())
        .set(tokens_dsl::revoked_at.eq(Some(Utc::now())))
        .execute(&mut conn)?;
    Ok(())
}

/// Looks up a presented raw token by its hash. Returns `None` if the token
/// is unknown, revoked, or past its expiry. On a valid match, best-effort
/// updates `last_used_at` — a failure to update must not fail the check.
pub fn verify(pool: &Pool, raw_token: &str) -> Result<Option<AgentToken>, AppError> {
    let hash = hash_token(raw_token);
    let mut conn = pool.get()?;

    let found = tokens_dsl::agent_tokens
        .filter(tokens_dsl::token_hash.eq(&hash))
        .select(AgentToken::as_select())
        .first::<AgentToken>(&mut conn)
        .optional()
        .map_err(AppError::from)?;

    let Some(token) = found else {
        return Ok(None);
    };

    if token.revoked_at.is_some() {
        return Ok(None);
    }
    if let Some(expires_at) = token.expires_at {
        if expires_at <= Utc::now() {
            return Ok(None);
        }
    }

    let _ = diesel::update(tokens_dsl::agent_tokens.find(token.id))
        .set(tokens_dsl::last_used_at.eq(Some(Utc::now())))
        .execute(&mut conn);

    Ok(Some(token))
}
```

- [ ] **Step 4: Register the module**

In `Server/src/services/mod.rs`, add to the alphabetically-ordered list (after `admin_service`, before `call_service`):

```rust
pub mod agent_token_service;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --all agent_token_service` (from `Server/`)
Expected: all 5 tests pass (`compute_expires_at_none_means_never_expires`, `compute_expires_at_adds_days`, `compute_expires_at_rejects_non_positive_days`, `normalize_name_trims_and_rejects_empty`, `token_prefix_is_stable_and_short`).

- [ ] **Step 6: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings` (from `Server/`)
Expected: no warnings from the new file.

- [ ] **Step 7: Commit**

```bash
cd Server
git add src/services/agent_token_service.rs src/services/mod.rs
git commit -m "feat(server): add agent_token_service"
```

---

### Task 4: Auth middleware — accept agent tokens on `AdminUser`

**Files:**
- Modify: `Server/src/auth/middleware.rs`

**Interfaces:**
- Consumes: `crate::services::agent_token_service::verify` (Task 3), `crate::auth::tokens::verify_access_token` (existing).
- Produces: `AdminUser::from_request` now accepts either a valid admin JWT or a valid `agt_`-prefixed agent token. `AuthUser::from_request` is unchanged (JWT-only).

- [ ] **Step 1: Read the current file to confirm line numbers before editing**

Run: `sed -n '1,82p' Server/src/auth/middleware.rs`
Expected: matches the file as shown in the spec's "Backend Changes" section — a sync `extract_claims(req) -> Result<Claims, AppError>` used by both extractors.

- [ ] **Step 2: Rewrite the file**

Replace the full contents of `Server/src/auth/middleware.rs` with:

```rust
use actix_web::{web, FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{agent_token_service, user_service};

use super::claims::Claims;
use super::tokens::verify_access_token;

/// Extractor for any authenticated user.
pub struct AuthUser(pub Claims);

/// Extractor for admin-only routes. Fails with 403 if role != "admin".
#[allow(dead_code)]
pub struct AdminUser(pub Claims);

fn bearer_token(req: &HttpRequest) -> Result<&str, AppError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    auth_header.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)
}

fn extract_claims(req: &HttpRequest) -> Result<Claims, AppError> {
    let config = req
        .app_data::<web::Data<Config>>()
        .ok_or(AppError::Unauthorized)?;

    let token = bearer_token(req)?;
    verify_access_token(token, &config.jwt_secret).map_err(|_| AppError::Unauthorized)
}

/// Result of authenticating an admin-route request: either a normal JWT
/// (still subject to the existing active-user + role DB check below) or an
/// agent token (already fully validated — active, non-revoked, non-expired
/// — by `agent_token_service::verify`, so it's trusted directly).
enum AdminAuth {
    Jwt(Claims),
    AgentToken(Claims),
}

/// Same as `extract_claims`, but for admin-only routes: if the bearer string
/// isn't a valid JWT and starts with `agt_`, it's checked against the
/// `agent_tokens` table instead.
async fn extract_admin_auth(req: &HttpRequest, pool: &Pool) -> Result<AdminAuth, AppError> {
    let config = req
        .app_data::<web::Data<Config>>()
        .ok_or(AppError::Unauthorized)?;
    let token = bearer_token(req)?;

    if let Ok(claims) = verify_access_token(token, &config.jwt_secret) {
        return Ok(AdminAuth::Jwt(claims));
    }

    if !token.starts_with("agt_") {
        return Err(AppError::Unauthorized);
    }

    let agent_token = agent_token_service::verify(pool, token)?.ok_or(AppError::Unauthorized)?;
    let now = chrono::Utc::now().timestamp();
    Ok(AdminAuth::AgentToken(Claims::new(
        agent_token.created_by,
        "admin".to_string(),
        now,
        now + 300,
    )))
}

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<AuthUser, AppError>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let claims = extract_claims(req);
        let pool = req.app_data::<web::Data<Pool>>().cloned();
        Box::pin(async move {
            let claims = claims?;
            let user_id = claims.user_id().map_err(|_| AppError::Unauthorized)?;
            let pool = pool.ok_or(AppError::Unauthorized)?;
            let user = user_service::find_by_id(&pool, user_id)?;
            match user {
                Some(u) if u.is_active => Ok(AuthUser(claims)),
                _ => Err(AppError::Unauthorized),
            }
        })
    }
}

impl FromRequest for AdminUser {
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<AdminUser, AppError>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let req = req.clone();
        let pool = req.app_data::<web::Data<Pool>>().cloned();
        Box::pin(async move {
            let pool = pool.ok_or(AppError::Unauthorized)?;
            let auth = extract_admin_auth(&req, &pool).await?;

            // Agent-token claims are already fully validated by
            // `extract_admin_auth` (active, non-revoked, non-expired) —
            // trusted directly, no further DB check needed.
            let claims = match auth {
                AdminAuth::AgentToken(claims) => return Ok(AdminUser(claims)),
                AdminAuth::Jwt(claims) => claims,
            };

            // JWT path: unchanged from the pre-existing behavior — still
            // requires the user row to exist and be active, and role == "admin".
            let user_id = claims.user_id().map_err(|_| AppError::Unauthorized)?;
            let user = user_service::find_by_id(&pool, user_id)?;
            match user {
                Some(u) if u.is_active => {
                    if claims.role == "admin" {
                        Ok(AdminUser(claims))
                    } else {
                        Err(AppError::Forbidden)
                    }
                }
                _ => Err(AppError::Unauthorized),
            }
        })
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check` (from `Server/`)
Expected: no errors. If `Claims::new` isn't `pub` or has a different signature, check `Server/src/auth/claims.rs` — it should already be `pub fn new(user_id: Uuid, role: String, iat: i64, exp: i64) -> Self`.

- [ ] **Step 4: Run existing auth tests to confirm no regression**

Run: `cargo test -p sync-server auth::` (from `Server/`)
Expected: all existing tests in `src/auth/tokens.rs` and `src/auth/claims.rs` still pass unchanged.

- [ ] **Step 5: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings` (from `Server/`)
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
cd Server
git add src/auth/middleware.rs
git commit -m "feat(server): accept agent tokens on AdminUser extractor"
```

---

### Task 5: Routes — create/list/revoke agent tokens

**Files:**
- Modify: `Server/src/routes/admin.rs`

**Interfaces:**
- Consumes: `agent_token_service::{create, list, revoke}` (Task 3), `admin_service::append_audit_log` (existing, signature `fn append_audit_log(pool: &Pool, actor_user_id: Option<Uuid>, action: &str, target: Option<&str>, details: serde_json::Value) -> Result<(), AppError>`).
- Produces: three new HTTP endpoints under `/api/admin/agent-tokens`, consumed by Task 6 (integration tests) and Task 7 (dashboard proxy).

- [ ] **Step 1: Add request/response types**

In `Server/src/routes/admin.rs`, add near the other `#[derive(Debug, Deserialize)]` request structs (e.g. right after `CreateServerNewsRequest`):

```rust
#[derive(Debug, Deserialize)]
pub struct CreateAgentTokenRequest {
    pub name: String,
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct AgentTokenCreatedView {
    pub id: Uuid,
    pub name: String,
    pub token: String,
    pub token_prefix: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, serde::Serialize)]
pub struct AgentTokenView {
    pub id: Uuid,
    pub name: String,
    pub token_prefix: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn to_agent_token_view(token: crate::models::AgentToken) -> AgentTokenView {
    AgentTokenView {
        id: token.id,
        name: token.name,
        token_prefix: token.token_prefix,
        created_at: token.created_at,
        expires_at: token.expires_at,
        last_used_at: token.last_used_at,
        revoked_at: token.revoked_at,
    }
}
```

(`AgentTokenView` deliberately has no `token` or `token_hash` field — the raw secret only ever appears in `AgentTokenCreatedView`, returned once from the create endpoint.)

- [ ] **Step 2: Add the handlers**

Add near the other handler functions (e.g. right after `delete_server_news`):

```rust
pub async fn create_agent_token(
    pool: web::Data<Pool>,
    admin: AdminUser,
    body: web::Json<CreateAgentTokenRequest>,
) -> Result<HttpResponse, AppError> {
    let admin_user_id = admin.0.user_id()?;
    let (created, raw_token) = agent_service::create(
        &pool,
        admin_user_id,
        &body.name,
        body.expires_in_days,
    )?;
    admin_service::append_audit_log(
        &pool,
        Some(admin_user_id),
        "agent_token.create",
        Some(&created.id.to_string()),
        serde_json::json!({
            "name": created.name,
            "token_prefix": created.token_prefix,
            "expires_at": created.expires_at,
        }),
    )?;
    Ok(HttpResponse::Created().json(AgentTokenCreatedView {
        id: created.id,
        name: created.name,
        token: raw_token,
        token_prefix: created.token_prefix,
        created_at: created.created_at,
        expires_at: created.expires_at,
    }))
}

pub async fn list_agent_tokens(
    pool: web::Data<Pool>,
    _admin: AdminUser,
) -> Result<HttpResponse, AppError> {
    let items = agent_service::list(&pool)?;
    Ok(HttpResponse::Ok().json(
        items
            .into_iter()
            .map(to_agent_token_view)
            .collect::<Vec<_>>(),
    ))
}

pub async fn revoke_agent_token(
    pool: web::Data<Pool>,
    admin: AdminUser,
    token_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let admin_user_id = admin.0.user_id()?;
    agent_service::revoke(&pool, *token_id)?;
    admin_service::append_audit_log(
        &pool,
        Some(admin_user_id),
        "agent_token.revoke",
        Some(&token_id.to_string()),
        serde_json::json!({}),
    )?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "revoked" })))
}
```

- [ ] **Step 3: Import the service under a short alias**

At the top of `Server/src/routes/admin.rs`, find the line:

```rust
use crate::services::{admin_service, guild_service, moderation_service, server_news_service};
```

Replace it with:

```rust
use crate::services::{
    admin_service, agent_token_service as agent_service, guild_service, moderation_service,
    server_news_service,
};
```

- [ ] **Step 4: Wire the routes into `configure`**

In the `pub fn configure(cfg: &mut web::ServiceConfig)` function at the bottom of the file, add these three `.route(...)` calls in the chain (e.g. right after the `.route("/server-news/{news_id}", web::delete().to(delete_server_news))` line — find it and insert after):

```rust
        .route("/agent-tokens", web::get().to(list_agent_tokens))
        .route("/agent-tokens", web::post().to(create_agent_token))
        .route(
            "/agent-tokens/{token_id}",
            web::delete().to(revoke_agent_token),
        )
```

Remember the chain ends with a `;` on its last call — make sure your inserted lines stay inside the `.route(...)` method-chain (each new call is `.route(...)`, not a new statement).

- [ ] **Step 5: Verify it compiles**

Run: `cargo check` (from `Server/`)
Expected: no errors.

- [ ] **Step 6: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings` (from `Server/`)
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
cd Server
git add src/routes/admin.rs
git commit -m "feat(server): add agent token admin endpoints"
```

---

### Task 6: Integration tests

**Files:**
- Create: `Server/tests/agent_tokens_tests.rs`

**Interfaces:**
- Consumes: the three endpoints from Task 5, following the exact `TEST_BASE_URL` / `TEST_ADMIN_TOKEN` convention already used in `Server/tests/guild_tests.rs`.

- [ ] **Step 1: Write the test file**

`Server/tests/agent_tokens_tests.rs`:

```rust
/// Integration tests for agent admin tokens.
///
/// These tests require a running server and real database.
/// Set `TEST_BASE_URL` (e.g. `http://localhost:8080`) to run them.
/// Admin-gated tests additionally require `TEST_ADMIN_TOKEN` — a JWT whose
/// `role` claim equals "admin". If either is absent, these tests are skipped.
use reqwest::Client;
use serde_json::{json, Value};

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn admin_token() -> Option<String> {
    std::env::var("TEST_ADMIN_TOKEN").ok()
}

#[tokio::test]
async fn agent_tokens_require_admin_auth() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let res = client
        .get(format!("{base}/api/admin/agent-tokens"))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn agent_token_full_lifecycle() {
    let (Some(base), Some(admin_tok)) = (base_url(), admin_token()) else {
        return;
    };
    let client = Client::new();

    // Create
    let create_res = client
        .post(format!("{base}/api/admin/agent-tokens"))
        .bearer_auth(&admin_tok)
        .json(&json!({ "name": "integration-test-agent" }))
        .send()
        .await
        .expect("create request failed");
    assert_eq!(create_res.status(), 201);
    let created: Value = create_res.json().await.expect("create json");
    let raw_token = created["token"].as_str().expect("missing token").to_string();
    let token_id = created["id"].as_str().expect("missing id").to_string();
    assert!(raw_token.starts_with("agt_"));

    // List shows the prefix, never the raw token
    let list_res = client
        .get(format!("{base}/api/admin/agent-tokens"))
        .bearer_auth(&admin_tok)
        .send()
        .await
        .expect("list request failed");
    assert_eq!(list_res.status(), 200);
    let list_body: Value = list_res.json().await.expect("list json");
    let entry = list_body
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == token_id)
        .expect("created token missing from list");
    assert!(entry.get("token").is_none());
    assert!(entry["token_prefix"].as_str().unwrap().starts_with("agt_"));

    // The raw agent token authenticates as admin
    let overview_res = client
        .get(format!("{base}/api/admin/overview"))
        .bearer_auth(&raw_token)
        .send()
        .await
        .expect("overview request failed");
    assert_eq!(overview_res.status(), 200);

    // Revoke
    let revoke_res = client
        .delete(format!("{base}/api/admin/agent-tokens/{token_id}"))
        .bearer_auth(&admin_tok)
        .send()
        .await
        .expect("revoke request failed");
    assert_eq!(revoke_res.status(), 200);

    // The revoked token no longer authenticates
    let after_revoke_res = client
        .get(format!("{base}/api/admin/overview"))
        .bearer_auth(&raw_token)
        .send()
        .await
        .expect("post-revoke overview request failed");
    assert_eq!(after_revoke_res.status(), 401);
}
```

- [ ] **Step 2: Run the tests (skipped if no local server)**

Run: `cargo test --test agent_tokens_tests` (from `Server/`)
Expected: both tests pass trivially (return early / no assertions run) if `TEST_BASE_URL` and `TEST_ADMIN_TOKEN` are unset, matching the existing behavior of `tests/guild_tests.rs`. This is fine to commit without a live server — CI or a manual run against a running instance is what exercises the assertions.

- [ ] **Step 3: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings` (from `Server/`)
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
cd Server
git add tests/agent_tokens_tests.rs
git commit -m "test(server): add agent token integration tests"
```

---

### Task 7: Dashboard proxy routes

**Files:**
- Create: `Server/dashboard/app/api/admin/agent-tokens/route.ts`
- Create: `Server/dashboard/app/api/admin/agent-tokens/[tokenId]/route.ts`

**Interfaces:**
- Consumes: `ACCESS_COOKIE`, `REFRESH_COOKIE`, `syncServerUrl` from `@/lib/server-api`; `assertSameOrigin` from `@/lib/security` (both existing).
- Produces: `GET /api/admin/agent-tokens`, `POST /api/admin/agent-tokens`, `DELETE /api/admin/agent-tokens/[tokenId]` — proxied to the Rust backend, consumed by Task 8's UI.

- [ ] **Step 1: Create the collection route**

`Server/dashboard/app/api/admin/agent-tokens/route.ts`:

```typescript
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, REFRESH_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type RefreshResponse = {
  access_token: string;
  refresh_token: string;
  expires_in: number;
};

const secure = process.env.NODE_ENV === "production";

type RequestContext = {
  access: string | null;
  refresh: string | null;
};

async function getRequestContext(): Promise<RequestContext> {
  const jar = await cookies();
  return {
    access: jar.get(ACCESS_COOKIE)?.value ?? null,
    refresh: jar.get(REFRESH_COOKIE)?.value ?? null,
  };
}

async function refreshTokens(refresh: string): Promise<RefreshResponse | null> {
  const refreshResponse = await fetch(syncServerUrl("/auth/refresh"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({ refresh_token: refresh }),
    cache: "no-store",
  });

  if (!refreshResponse.ok) {
    return null;
  }

  return (await refreshResponse.json()) as RefreshResponse;
}

async function proxyAgentTokens(
  method: "GET" | "POST",
  accessToken: string,
  payload?: unknown,
): Promise<Response> {
  return fetch(syncServerUrl("/api/admin/agent-tokens"), {
    method,
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: payload == null ? undefined : JSON.stringify(payload),
    cache: "no-store",
  });
}

async function withRefreshRetry(
  method: "GET" | "POST",
  context: RequestContext,
  payload?: unknown,
): Promise<{ response: Response; refreshed: RefreshResponse | null }> {
  if (!context.access) {
    return { response: new Response(null, { status: 401 }), refreshed: null };
  }

  let response = await proxyAgentTokens(method, context.access, payload);
  if (response.status !== 401 || !context.refresh) {
    return { response, refreshed: null };
  }

  const refreshed = await refreshTokens(context.refresh);
  if (!refreshed) {
    return { response, refreshed: null };
  }

  response = await proxyAgentTokens(method, refreshed.access_token, payload);
  return { response, refreshed };
}

function withUpdatedCookies(next: NextResponse, refreshed: RefreshResponse | null): NextResponse {
  if (!refreshed) {
    return next;
  }

  next.cookies.set(ACCESS_COOKIE, refreshed.access_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: refreshed.expires_in,
  });
  next.cookies.set(REFRESH_COOKIE, refreshed.refresh_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: 60 * 60 * 24 * 30,
  });
  return next;
}

export async function GET() {
  const context = await getRequestContext();
  const { response, refreshed } = await withRefreshRetry("GET", context);

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json(
      { error: body || "Failed to load agent tokens" },
      { status: response.status === 401 ? 401 : 400 },
    );
  }

  const next = NextResponse.json(await response.json());
  return withUpdatedCookies(next, refreshed);
}

export async function POST(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const payload = await request.json();
  const context = await getRequestContext();
  const { response, refreshed } = await withRefreshRetry("POST", context, payload);

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json(
      { error: body || "Failed to create agent token" },
      { status: response.status === 401 ? 401 : 400 },
    );
  }

  const next = NextResponse.json(await response.json(), { status: 201 });
  return withUpdatedCookies(next, refreshed);
}
```

- [ ] **Step 2: Create the item route**

`Server/dashboard/app/api/admin/agent-tokens/[tokenId]/route.ts`:

```typescript
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, REFRESH_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type RefreshResponse = {
  access_token: string;
  refresh_token: string;
  expires_in: number;
};

const secure = process.env.NODE_ENV === "production";

type RequestContext = {
  access: string | null;
  refresh: string | null;
};

type Params = { params: Promise<{ tokenId: string }> };

async function getRequestContext(): Promise<RequestContext> {
  const jar = await cookies();
  return {
    access: jar.get(ACCESS_COOKIE)?.value ?? null,
    refresh: jar.get(REFRESH_COOKIE)?.value ?? null,
  };
}

async function refreshTokens(refresh: string): Promise<RefreshResponse | null> {
  const refreshResponse = await fetch(syncServerUrl("/auth/refresh"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({ refresh_token: refresh }),
    cache: "no-store",
  });

  if (!refreshResponse.ok) {
    return null;
  }

  return (await refreshResponse.json()) as RefreshResponse;
}

async function proxyAgentToken(
  method: "DELETE",
  tokenId: string,
  accessToken: string,
): Promise<Response> {
  return fetch(syncServerUrl(`/api/admin/agent-tokens/${tokenId}`), {
    method,
    headers: {
      Authorization: `Bearer ${accessToken}`,
      Accept: "application/json",
    },
    cache: "no-store",
  });
}

async function withRefreshRetry(
  tokenId: string,
  context: RequestContext,
): Promise<{ response: Response; refreshed: RefreshResponse | null }> {
  if (!context.access) {
    return { response: new Response(null, { status: 401 }), refreshed: null };
  }

  let response = await proxyAgentToken("DELETE", tokenId, context.access);
  if (response.status !== 401 || !context.refresh) {
    return { response, refreshed: null };
  }

  const refreshed = await refreshTokens(context.refresh);
  if (!refreshed) {
    return { response, refreshed: null };
  }

  response = await proxyAgentToken("DELETE", tokenId, refreshed.access_token);
  return { response, refreshed };
}

function withUpdatedCookies(next: NextResponse, refreshed: RefreshResponse | null): NextResponse {
  if (!refreshed) {
    return next;
  }

  next.cookies.set(ACCESS_COOKIE, refreshed.access_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: refreshed.expires_in,
  });
  next.cookies.set(REFRESH_COOKIE, refreshed.refresh_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: 60 * 60 * 24 * 30,
  });
  return next;
}

export async function DELETE(request: Request, { params }: Params) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const { tokenId } = await params;
  const context = await getRequestContext();
  const { response, refreshed } = await withRefreshRetry(tokenId, context);

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json(
      { error: body || "Failed to revoke agent token" },
      { status: response.status === 401 ? 401 : response.status === 404 ? 404 : 400 },
    );
  }

  const next = NextResponse.json(await response.json());
  return withUpdatedCookies(next, refreshed);
}
```

- [ ] **Step 3: Type-check**

Run: `cd Server/dashboard && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Lint**

Run: `cd Server/dashboard && pnpm lint`
Expected: no errors on the two new files.

- [ ] **Step 5: Commit**

```bash
cd Server/dashboard
git add app/api/admin/agent-tokens
git commit -m "feat(dashboard): add agent token proxy routes"
```

---

### Task 8: Dashboard page + UI panel

**Files:**
- Create: `Server/dashboard/app/(admin)/agent-access/page.tsx`
- Create: `Server/dashboard/app/(admin)/agent-access/ui/agent-access-panel.tsx`

**Interfaces:**
- Consumes: `apiGetJson` and `requireAdminSession` (existing, from `@/lib/server-api` and `@/lib/session`); the proxy routes from Task 7 (`/api/admin/agent-tokens`, `/api/admin/agent-tokens/[id]`); `Alert`, `AlertDescription`, `Button`, `Input`, `Label`, `Separator`, `Badge` from `@/components/ui/*` (existing shadcn components, already used by `planet-news-form.tsx` and `layout.tsx`).
- Produces: the `/agent-access` page, wired into nav by Task 9.

- [ ] **Step 1: Create the server component page**

`Server/dashboard/app/(admin)/agent-access/page.tsx`:

```tsx
import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { AgentAccessPanel, type AgentTokenView } from "./ui/agent-access-panel";

export default async function AgentAccessPage() {
  await requireAdminSession();
  const tokens = await apiGetJson<AgentTokenView[]>("/api/admin/agent-tokens");

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Agent Access</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Issue a token so an AI agent can connect to this server and manage content
          (planet settings, news, stickers) on your behalf. Agent tokens have full
          admin access — treat them like a password.
        </p>
      </div>
      <AgentAccessPanel initialTokens={tokens} />
    </div>
  );
}
```

- [ ] **Step 2: Create the client component**

`Server/dashboard/app/(admin)/agent-access/ui/agent-access-panel.tsx`:

```tsx
"use client";

import { useState } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";

export type AgentTokenView = {
  id: string;
  name: string;
  token_prefix: string;
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
  revoked_at: string | null;
};

type CreatedToken = {
  id: string;
  name: string;
  token: string;
  token_prefix: string;
  created_at: string;
  expires_at: string | null;
};

const EXPIRY_OPTIONS = [
  { value: "", label: "Never" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
];

function tokenStatus(token: AgentTokenView): { label: string; variant: "outline" | "destructive" | "secondary" } {
  if (token.revoked_at) {
    return { label: "Revoked", variant: "destructive" };
  }
  if (token.expires_at && new Date(token.expires_at).getTime() <= Date.now()) {
    return { label: "Expired", variant: "secondary" };
  }
  return { label: "Active", variant: "outline" };
}

export function AgentAccessPanel({ initialTokens }: { initialTokens: AgentTokenView[] }) {
  const [name, setName] = useState("");
  const [expiryDays, setExpiryDays] = useState("");
  const [items, setItems] = useState(initialTokens);
  const [creating, setCreating] = useState(false);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [justCreated, setJustCreated] = useState<CreatedToken | null>(null);
  const [copied, setCopied] = useState(false);

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setCreating(true);
    setError(null);
    setCopied(false);

    try {
      const response = await fetch("/api/admin/agent-tokens", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name,
          expires_in_days: expiryDays ? Number(expiryDays) : null,
        }),
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as { error?: string } | null;
        setError(body?.error ?? "Failed to create agent token");
        return;
      }

      const created = (await response.json()) as CreatedToken;
      setJustCreated(created);
      setItems((prev) => [
        {
          id: created.id,
          name: created.name,
          token_prefix: created.token_prefix,
          created_at: created.created_at,
          expires_at: created.expires_at,
          last_used_at: null,
          revoked_at: null,
        },
        ...prev,
      ]);
      setName("");
      setExpiryDays("");
    } catch {
      setError("Failed to create agent token");
    } finally {
      setCreating(false);
    }
  }

  async function onRevoke(token: AgentTokenView) {
    if (!confirm(`Revoke "${token.name}"? Any agent using it will immediately lose access.`)) {
      return;
    }
    setRevokingId(token.id);
    setError(null);

    try {
      const response = await fetch(`/api/admin/agent-tokens/${token.id}`, {
        method: "DELETE",
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as { error?: string } | null;
        setError(body?.error ?? "Failed to revoke agent token");
        return;
      }

      setItems((prev) =>
        prev.map((item) =>
          item.id === token.id ? { ...item, revoked_at: new Date().toISOString() } : item,
        ),
      );
    } catch {
      setError("Failed to revoke agent token");
    } finally {
      setRevokingId(null);
    }
  }

  async function copyToken() {
    if (!justCreated) {
      return;
    }
    await navigator.clipboard.writeText(justCreated.token);
    setCopied(true);
  }

  return (
    <div className="space-y-8">
      {justCreated ? (
        <Alert>
          <AlertDescription className="space-y-3">
            <p className="font-medium text-foreground">
              Token created for &ldquo;{justCreated.name}&rdquo;. Copy it now — it will not be
              shown again.
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <code className="rounded bg-muted px-2 py-1 text-xs">{justCreated.token}</code>
              <Button onClick={() => void copyToken()} size="sm" type="button" variant="outline">
                {copied ? "Copied" : "Copy"}
              </Button>
              <Button onClick={() => setJustCreated(null)} size="sm" type="button" variant="ghost">
                Dismiss
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Have the agent send this in every admin API request as{" "}
              <code className="rounded bg-muted px-1">Authorization: Bearer {justCreated.token_prefix}…</code>
            </p>
          </AlertDescription>
        </Alert>
      ) : null}

      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          New agent token
        </p>
        <form className="space-y-4" onSubmit={onSubmit}>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="agent-token-name">Name</Label>
              <Input
                id="agent-token-name"
                maxLength={120}
                onChange={(event) => setName(event.target.value)}
                placeholder="Claude ops agent"
                required
                type="text"
                value={name}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="agent-token-expiry">Expires</Label>
              <select
                className="border-input flex h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs"
                id="agent-token-expiry"
                onChange={(event) => setExpiryDays(event.target.value)}
                value={expiryDays}
              >
                {EXPIRY_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {error ? (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}

          <Button disabled={creating} type="submit">
            {creating ? "Creating…" : "Create token"}
          </Button>
        </form>
      </section>

      <Separator />

      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Tokens <span className="normal-case font-normal text-muted-foreground/50">({items.length})</span>
        </p>

        {items.length === 0 ? (
          <p className="text-sm text-muted-foreground">No agent tokens issued yet.</p>
        ) : (
          <div className="divide-y rounded-lg border">
            {items.map((item) => {
              const status = tokenStatus(item);
              const isRevoked = Boolean(item.revoked_at);
              return (
                <div className="flex items-center justify-between gap-4 px-4 py-3" key={item.id}>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <p className="font-medium leading-tight">{item.name}</p>
                      <Badge variant={status.variant}>{status.label}</Badge>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground/60">
                      {item.token_prefix}… · created {new Date(item.created_at).toLocaleString()}
                      {item.expires_at ? ` · expires ${new Date(item.expires_at).toLocaleString()}` : ""}
                      {item.last_used_at ? ` · last used ${new Date(item.last_used_at).toLocaleString()}` : ""}
                    </p>
                  </div>
                  <Button
                    disabled={isRevoked || revokingId === item.id}
                    onClick={() => void onRevoke(item)}
                    size="sm"
                    type="button"
                    variant="destructive"
                  >
                    {revokingId === item.id ? "Revoking…" : isRevoked ? "Revoked" : "Revoke"}
                  </Button>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}
```

- [ ] **Step 3: Confirm the `Badge` component's variant prop supports `"outline" | "destructive" | "secondary"`**

Run: `grep -n "variant" Server/dashboard/components/ui/badge.tsx`
Expected: a `cva` variants object including at least `outline`, `destructive`, and `secondary`. If the exact variant names differ, adjust `tokenStatus`'s return type and usages in Step 2 to match the real ones (do not invent variants the component doesn't support).

- [ ] **Step 4: Type-check and lint**

Run: `cd Server/dashboard && npx tsc --noEmit && pnpm lint`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
cd Server/dashboard
git add "app/(admin)/agent-access"
git commit -m "feat(dashboard): add agent access page"
```

---

### Task 9: Wire into navigation and final verification

**Files:**
- Modify: `Server/dashboard/app/(admin)/layout.tsx`

**Interfaces:**
- Consumes: nothing new.
- Produces: nothing consumed by later tasks — this is the last task.

- [ ] **Step 1: Add the nav entry**

In `Server/dashboard/app/(admin)/layout.tsx`, find:

```typescript
  { href: "/planet-news", label: "Planet News" },
  { href: "/audit", label: "Audit Logs" },
```

Replace with:

```typescript
  { href: "/planet-news", label: "Planet News" },
  { href: "/agent-access", label: "Agent Access" },
  { href: "/audit", label: "Audit Logs" },
```

- [ ] **Step 2: Full backend verification**

Run (from `Server/`):

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Expected: all pass. (Live-server-gated tests in `tests/agent_tokens_tests.rs` and `tests/guild_tests.rs` no-op without `TEST_BASE_URL` — that's expected, not a failure.)

- [ ] **Step 3: Full dashboard verification**

Run (from `Server/dashboard/`):

```bash
npx tsc --noEmit
pnpm lint
pnpm build
```

Expected: all pass, including a successful production build (this catches server/client component boundary mistakes that `tsc`/`lint` alone can miss).

- [ ] **Step 4: Manual smoke test (requires a running server + dashboard)**

If a local Postgres + server + dashboard can be brought up (check `Server/docker-compose.yml` and `Server/dashboard/README.md` for the exact commands — do not guess), log into the dashboard as an admin, visit `/agent-access`, create a token, confirm the raw token is shown once and copyable, confirm it disappears on reload with only the prefix remaining in the list, then use the raw token with `curl` against `GET /api/admin/overview` on the live server to confirm it authenticates, then revoke it in the UI and confirm the same `curl` now returns 401. If no local environment is available, state that explicitly rather than claiming this step was verified.

- [ ] **Step 5: Commit**

```bash
cd Server/dashboard
git add "app/(admin)/layout.tsx"
git commit -m "feat(dashboard): add agent access nav entry"
```
