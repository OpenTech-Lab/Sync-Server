# Agent Admin Tokens — Design

**Date:** 2026-08-15
**Status:** Approved

## Problem

The server owner wants to let an external AI agent connect to the Sync server's admin API (to set up or manage content — planet config, news, stickers, etc.) without sharing their own login. There is currently no credential type for this: the only auth path is a short-lived JWT issued at human login (`src/auth/tokens.rs`, `src/auth/middleware.rs`).

## Decisions (confirmed with user)

1. **Scope:** an agent token has **full admin power** — identical to a human admin JWT. No reduced-scope role for v1.
2. **Transport:** `Authorization: Bearer <token>`, the same header/scheme as JWTs. The existing `AdminUser` extractor is extended to also accept this token type; no new header or auth path.
3. **Lifecycle:** issue, revoke, and optional expiry (30/90 days, or none). No rotation.

## Token Format

`agt_<64-hex-chars>` — 32 random bytes, hex-encoded, prefixed with `agt_` so it's visually distinguishable from a JWT (which contains dots) in logs and headers.

Reuses the existing crypto helpers in `src/auth/tokens.rs`:
- `generate_refresh_token()` → `(raw, sha256_hex_hash)` for the random bytes + hash
- `hash_token(raw)` → sha256 hex, for verifying a presented token

Only the SHA-256 hash is ever stored. The raw token is shown to the admin **exactly once**, at creation time, in the dashboard UI.

## Data Model

New table `agent_tokens`, modeled directly on the existing `refresh_tokens` table (`src/models/refresh_token.rs`, `src/schema.rs`):

| Column | Type | Notes |
|---|---|---|
| `id` | Uuid | PK |
| `name` | Text | admin-supplied label, e.g. "Claude ops agent" |
| `token_hash` | Text | sha256 hex, unique |
| `token_prefix` | Text | first 12 chars of the raw token (`agt_` + 8 hex chars) — shown in the list UI so an admin can tell tokens apart without ever re-revealing the secret |
| `created_by` | Uuid | FK → `users.id`, the admin who issued it |
| `created_at` | Timestamptz | default now() |
| `expires_at` | Timestamptz, nullable | null = never expires |
| `last_used_at` | Timestamptz, nullable | updated best-effort on successful auth |
| `revoked_at` | Timestamptz, nullable | null = active |

## Backend Changes (`Server/`)

- **Migration:** `migrations/2026-08-15-000026_agent_tokens/` (up/down SQL creating the table above).
- **Model:** `src/models/agent_token.rs` — `AgentToken` (Queryable/Selectable/Identifiable) + `NewAgentToken` (Insertable), following `refresh_token.rs` exactly. Registered in `src/models/mod.rs`.
- **Service:** `src/services/agent_token_service.rs`:
  - `create(pool, created_by: Uuid, name: &str, expires_in_days: Option<i64>) -> Result<(AgentToken, String))` — generates the token, hashes it, inserts, returns the row plus the **raw** token (caller must not persist the raw value beyond the HTTP response).
  - `list(pool) -> Result<Vec<AgentToken>>` — ordered by `created_at desc`.
  - `revoke(pool, id: Uuid) -> Result<()>` — sets `revoked_at = now()` if not already revoked. Idempotent.
  - `verify(pool, raw_token: &str) -> Result<Option<AgentToken>>` — hashes the input, looks up by `token_hash`, returns `None` if missing/revoked/expired; on success, best-effort updates `last_used_at` (failure to update must not fail the auth check).
  - Registered in `src/services/mod.rs`.
- **Auth:** `src/auth/middleware.rs::extract_claims` becomes async and gains a fallback path used only by the `AdminUser` extractor (not `AuthUser`):
  1. Try `verify_access_token` (JWT) as today.
  2. If that fails **and** the bearer string starts with `agt_`, call `agent_token_service::verify`. On a valid, non-revoked, non-expired match, synthesize `Claims { sub: token.created_by.to_string(), role: "admin".into(), iat: now, exp: now + 300 }` (short synthetic `exp` — it's not persisted or checked again, just satisfies the `Claims` shape).
  3. Otherwise, `Unauthorized`.
  - `AuthUser` keeps calling the JWT-only path — agent tokens cannot authenticate as a regular user, only as admin.
- **Routes:** added directly to `src/routes/admin.rs` (which already holds every other `/api/admin/*` resource — server-news, config, guild, moderation, etc. — so this follows the file's existing organization rather than splitting it out), mounted under `/api/admin/agent-tokens` via the existing `configure()` function, all behind the existing `AdminUser` extractor:
  - `POST /api/admin/agent-tokens` — body `{ name: String, expires_in_days: Option<i64> }` → 201 `{ id, name, token, token_prefix, created_at, expires_at }` (the only response that ever contains `token`).
  - `GET /api/admin/agent-tokens` → 200 `[{ id, name, token_prefix, created_at, expires_at, last_used_at, revoked_at }]` (no `token_hash`, no `token`).
  - `DELETE /api/admin/agent-tokens/{id}` → 204, calls `revoke`.

## Dashboard Changes (`Server/dashboard/`)

Follows the existing `planet-news` page/proxy pattern exactly (`app/(admin)/planet-news/*`, `app/api/admin/server-news/*`).

- `app/(admin)/agent-access/page.tsx` — server component: `requireAdminSession()`, `apiGetJson("/api/admin/agent-tokens")`, renders `<AgentAccessPanel initialTokens={...} />`.
- `app/(admin)/agent-access/ui/agent-access-panel.tsx` — client component:
  - Table of existing tokens: name, `token_prefix`, created date, expiry, last used, status (active/expired/revoked badge), revoke button.
  - Create form: name (required) + expiry select (Never / 30 days / 90 days).
  - On successful create, shows the raw token **once** in a dismissible banner with a copy-to-clipboard button and explicit "this won't be shown again" warning, plus a short usage snippet: `Authorization: Bearer agt_...` against the server's public API base URL.
- `app/api/admin/agent-tokens/route.ts` (GET, POST) and `app/api/admin/agent-tokens/[tokenId]/route.ts` (DELETE) — proxy routes copied from the `server-news` proxy (cookie-based dashboard session, JWT refresh-and-retry, `assertSameOrigin` on mutating requests).
- `app/(admin)/layout.tsx` — add `{ href: "/agent-access", label: "Agent Access" }` to `navItems`.

## Testing

- Rust unit tests in `src/services/agent_token_service.rs` (or a `#[cfg(test)] mod tests` block there): create/verify roundtrip, revoked token rejected, expired token rejected, `last_used_at` updates on verify.
- Rust integration test in `tests/admin_tests.rs` (or a new `tests/agent_tokens_tests.rs`): an issued `agt_` token successfully authenticates against an existing `AdminUser`-gated route (e.g. `GET /api/admin/overview`); a revoked token gets 401.
- No new dashboard test infra — none of the sibling admin pages (`planet-news`, `config`, `stickers`) have component tests, so this stays consistent.

## Out of Scope (v1)

- Scoped/reduced-permission tokens (content-only, etc.) — explicitly deferred; full-admin only for now.
- Token rotation.
- Rate limiting specific to agent tokens (falls under whatever global rate limiting already exists, unchanged).
