# Phase 6 Threat Model (Cross-Cutting)

Date: 2026-03-02

## Scope
- Auth and session handling (`/auth/*`, dashboard session endpoints)
- Federation ingest and outbound delivery (ActivityPub + HTTP signatures)
- Mobile local storage, encrypted backup, and transport
- Admin dashboard controls and audit trails
- Infrastructure boundary (PostgreSQL, Redis, Docker deployment)

## Assumptions
- Public traffic terminates at TLS-capable reverse proxy in production.
- Server runs with `APP_ENV=production` and `ENFORCE_HTTPS=true`.
- JWT secrets and encryption keys are provisioned from secure secret stores.

## Key Assets
- JWT access/refresh tokens
- User account credentials and role claims
- Message contents and metadata
- Federation signing key material
- Admin configuration + audit trails
- Backup encryption keys and backup artifacts

## Primary Threats and Mitigations

### 1) Credential abuse and token replay
- Threat: brute-force login, leaked refresh token replay.
- Mitigations:
  - Route-level auth rate limits.
  - Refresh-token rotation and family replay revocation.
  - Short-lived access tokens.

### 2) Federation spoofing and replay
- Threat: forged inbound activities, replayed signed requests.
- Mitigations:
  - HTTP signature + digest validation.
  - Replay window checks.
  - Idempotency keying by activity id.
  - Host denylist + inbox rate limiting.

### 3) Unauthorized admin operations
- Threat: privilege escalation or CSRF against admin actions.
- Mitigations:
  - Admin role guard on server routes.
  - Same-origin protection for mutating dashboard APIs.
  - Login IP rate limiting.
  - Audit logging for admin action surface.

### 4) Data exposure at rest
- Threat: local/device or backup file extraction.
- Mitigations:
  - SQLCipher encrypted local DB on mobile.
  - Backup file AES-GCM encryption with key in secure storage.
  - No plaintext chat content persisted in backup artifacts.

### 5) Availability degradation
- Threat: sustained load or dependency exhaustion.
- Mitigations:
  - Rate-limit middleware on API/auth/federation ingress.
  - Redis pub/sub decoupling for realtime fan-out.
  - Baseline load test script and measurable p95 checks.

## Residual Risk / Follow-up
- Add periodic red-team style scenario testing around federation and dashboard auth.
- Integrate push provider registration endpoint once server-side token API is finalized.
