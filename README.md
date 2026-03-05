# Sync Server

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub Repo](https://img.shields.io/badge/GitHub-sync-blue?logo=github)](https://github.com/OpenTech-Lab/sync-server)

Sync Server is the backend component of the open-source, privacy-focused Sync chat application. It provides a decentralized, federated messaging platform inspired by Mastodon and Matrix, using ActivityPub for interoperability between servers (called "planets"). The server handles secure API endpoints for message sending, user management, and optional encrypted data backups. Emphasizing security and user control, chat data is primarily stored locally on client devices, with server-side storage only for opted-in, encrypted backups (similar to Signal).

This repo focuses on the server-side: Rust-based API, Next.js dashboard for administration, and easy deployment via Docker Compose. For the mobile client, see the companion repo: [sync-mobile](https://github.com/OpenTech-Lab/sync-mobile).

![Sync](docs/images/sync-banner.png)

## How to use
### 1. Prerequisites
- Docker Engine + Docker Compose plugin
- Git
- (Production) A public domain pointing to your server

### 2. Clone and prepare env
```bash
git clone https://github.com/OpenTech-Lab/sync-server.git
cd sync-server
cp .env.example .env
```

Edit `.env` at minimum:
- `POSTGRES_PASSWORD` (any strong value)
- `JWT_SECRET` (generate: `openssl rand -hex 32`)
- `INSTANCE_NAME`
- `ADMIN_EMAIL`
- `INSTANCE_DOMAIN`
  - local/dev HTTP: set to `localhost`
  - production HTTPS/federation: set to your real public domain

Optional:
- `RESEND_API_KEY`, `RESEND_FROM_EMAIL` (leave empty in local dev; password-reset emails are skipped)
- Push delivery mode:
  - `PUSH_DELIVERY_MODE=relay` (default, recommended for open/public servers)
    - all push events are forwarded to `notification_webhook_url` (hosted relay)
    - if `notification_webhook_url` is empty, server auto-uses `https://push.{INSTANCE_DOMAIN}/v1/push/webhook` (not applied for localhost/IP instance domains)
  - `PUSH_DELIVERY_MODE=direct`
    - iOS is sent directly to APNs from this server
  - `PUSH_DELIVERY_MODE=hybrid`
    - iOS tries direct APNs first, then falls back to webhook relay
- APNs direct push credentials (only needed for `direct` / `hybrid`):
  - `APNS_TEAM_ID`
  - `APNS_KEY_ID`
  - `APNS_BUNDLE_ID` (must match iOS app bundle id)
  - `APNS_PRIVATE_KEY_P8` (raw PEM with `\n` escapes or base64-encoded PEM)
  - `APNS_USE_SANDBOX=true` for dev builds, `false` for production/TestFlight
- Webhook host restriction (security default):
  - by default, `notification_webhook_url` host must equal `push.{INSTANCE_DOMAIN}`
  - external relay hosts are not allowed

### 3. Start stack (local/dev)
```bash
docker compose up -d
docker compose ps
```

Health checks:
```bash
curl http://localhost/health
curl http://localhost/ready
```

Access:
- API via nginx: `http://localhost/api/...`
- Dashboard/Auth: `http://localhost/login` (after login: `/dashboard`)

### 4. Initialize HTTPS
#### 4.1 Localhost HTTPS (development)
Set `INSTANCE_DOMAIN=localhost` in `.env`, then run:
```bash
bash scripts/init-ssl-dev.sh
```

Open:
- `https://localhost`
- `https://localhost/login`

#### 4.2 Production domain HTTPS (Let's Encrypt)
When `INSTANCE_DOMAIN` is a real domain with DNS A/AAAA pointing to this host:
- Ensure both DNS records point to this server:
  - `INSTANCE_DOMAIN`
  - `push.INSTANCE_DOMAIN`
```bash
bash scripts/init-ssl.sh
```

Then access:
- `https://<INSTANCE_DOMAIN>`
- `https://<INSTANCE_DOMAIN>/login`

### 5. Common operations
```bash
# View logs
docker compose logs -f api
docker compose logs -f dashboard
docker compose logs -f nginx

# Restart services
docker compose restart api dashboard nginx

# Stop stack
docker compose down
```

### Notes
- `scripts/init-ssl-dev.sh` is for local dev (`INSTANCE_DOMAIN=localhost`).
- `scripts/init-ssl.sh` is for real domains with Let's Encrypt.
- If `mkcert` is unavailable, `init-ssl-dev.sh` falls back to self-signed `openssl` certs (browser warning expected).
- For OSS/public infra, keep `PUSH_DELIVERY_MODE=relay` and configure `notification_webhook_url` to your hosted push relay.
- Direct APNs modes (`direct`, `hybrid`) require APNs credentials and iOS Push Notifications capability enabled for the app id/profile.

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

- Focus on security: Include tests for JWT, encryption, and API.
- Report issues on GitHub.
- PRs for Rust crates, Next.js features, or Docker improvements.

## License

MIT License - see [LICENSE](LICENSE).

## Acknowledgments

- Built on open-source tools: Rust ecosystem, ActivityPub spec, PostgreSQL/Redis.
- Inspired by Mastodon, Matrix, and Signal for federation and security.

For support, open an issue or join a community planet! 🚀
