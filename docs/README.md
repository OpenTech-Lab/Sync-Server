# Sync Server

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub Repo](https://img.shields.io/badge/GitHub-sync-blue?logo=github)](https://github.com/OpenTech-Lab/sync-server)

Sync Server is the backend component of the open-source, privacy-focused Sync chat application. It provides a decentralized, federated messaging platform inspired by Mastodon and Matrix, using ActivityPub for interoperability between servers (called "planets"). The server handles secure API endpoints for message sending, user management, and optional encrypted data backups. Emphasizing security and user control, chat data is primarily stored locally on client devices, with server-side storage only for opted-in, encrypted backups (similar to Signal).

This repo focuses on the server-side: Rust-based API, Next.js dashboard for administration, and easy deployment via Docker Compose. For the mobile client, see the companion repo: [sync-mobile](https://github.com/OpenTech-Lab/sync-mobile).

## Features

- **Federated Messaging**: Servers communicate via ActivityPub, enabling cross-planet chats (e.g., user@planetA to user@planetB).
- **Secure API**: High-security endpoints for real-time message relay, with end-to-end encryption enforced client-side.
- **Privacy-Centric Storage**: Optional encrypted backups of chat data; server never stores plain text.
- **User Limits and Auto-Scaling Advice**: Configurable max users per server, with automatic suggestions based on host PC specs (CPU/RAM detection).
- **Real-Time Notifications**: Push notifications for chats and mobile devices via webhooks and services like FCM/APNS.
- **Admin Dashboard**: Next.js-based web interface with WordPress-like login system for managing users, monitoring, and configurations.
- **Authentication and Security**: JWT-based auth for API and dashboard access.
- **Easy Self-Hosting**: Deploy with Docker Compose on any compatible hardware for communities or personal use.

## Security & Reliability Runbooks (Phase 6)

- Threat model: `docs/phase6-threat-model.md`
- Backup/restore drill: `docs/backup-restore-drill.md`
- Load baseline procedure: `docs/load-baseline.md`
- Encryption guarantees verification: `docs/encryption-guarantees.md`
- Local scan script: `scripts/security_scan.sh`

## Architecture

- **Backend**: Rust (using Actix Web or Rocket for the API server). Handles ActivityPub federation, secure message routing, and integrations.
- **Dashboard**: Next.js (React/TypeScript) for a user-friendly admin panel with login/auth similar to WordPress (e.g., session-based or JWT).
- **Data Flow**:
  - API receives/sends messages via secure endpoints (HTTPS, rate-limited).
  - Federation: ActivityPub for discovering and routing to other servers.
  - Notifications: Webhooks for external services; Redis pub/sub for internal real-time events like typing indicators.
  - Storage: Metadata (users, planets) in PostgreSQL; caches and queues in Redis; encrypted backups in PostgreSQL blobs.
- **Security**: All data encrypted at rest (via PostgreSQL extensions or app-level); JWT for auth tokens; webhook validation for notifications.

## Tech Stack

- **Backend Language**: Rust (with crates like actix-web, activitypub_federation, diesel for ORM).
- **Database**: PostgreSQL (for persistent data like users, metadata, and encrypted backups).
- **Caching/Real-Time**: Redis (for pub/sub messaging, caching sessions, and queues for notifications).
- **Authentication**: JWT (via jsonwebtoken crate in Rust; integrated with Next.js for dashboard).
- **Dashboard**: Next.js (with NextAuth or custom JWT for login system).
- **Notifications**: Webhook support (e.g., for FCM/APNS push notifications to mobile devices); Redis for in-app real-time events like chat updates.
- **Other Packages/Crates**:
  - Rust: ring or libsodium for crypto; reqwest for HTTP; serde for JSON; r2d2 for connection pooling.
  - Next.js: axios for API calls; tailwindcss for styling.
  - Federation: activitystreams-rs or similar for ActivityPub.
  - Deployment: Docker Compose (containers for Rust API, Next.js, PostgreSQL, Redis).
- **Encryption**: AES-256 for backups; end-to-end via client libs (server relays only).

## Getting Started

### Prerequisites

- Docker and Docker Compose.
- Rust toolchain (if building/customizing outside Docker).
- Node.js (for Next.js dashboard development).
- A domain name for production (required for federation and HTTPS).
- Git and basic CLI knowledge.
- Optional: FCM/APNS keys for push notifications.

### Installation

1. **Clone the Repo**:
   ```
   git clone https://github.com/OpenTech-Lab/sync-server.git
   cd sync-server
   ```

2. **Configure Environment**:
   - Copy `.env.example` to `.env` and populate values:
     - Database: `POSTGRES_DB`, `POSTGRES_USER`, `POSTGRES_PASSWORD`.
     - Redis: `REDIS_URL`.
     - JWT: `JWT_SECRET`.
     - Domain: `SERVER_DOMAIN` (e.g., your-planet.com).
     - Notifications: `FCM_SERVER_KEY`, `APNS_CERT` (for push).
     - Max Users: `MAX_USERS` (override auto-detect).
     - ActivityPub: Federation keys if needed.
   - For auto user limit: The system detects host specs on startup and logs a suggested max (e.g., based on CPU cores and RAM).

3. **Build and Run with Docker Compose**:
   ```
   docker-compose build
   docker-compose up -d
   ```
   - This launches:
     - Rust API container (port 8080).
     - Next.js dashboard (port 3000).
     - PostgreSQL (port 5432).
     - Redis (port 6379).
   - Migrations run automatically on start (using Diesel CLI in Rust).

4. **Setup Domain and SSL**:
   - Point your domain to the server IP.
   - Use Let's Encrypt (integrated in Docker Compose via certbot or similar) for HTTPS.
   - Update `.env` with domain for ActivityPub webfinger.

5. **Access the Dashboard**:
   - Visit `https://your-domain:3000` (or localhost:3000 for dev).
   - Default login: admin/admin (change immediately via dashboard).
   - Manage users, view logs, adjust max users, configure webhooks for notifications.

6. **API Usage**:
   - Secure endpoints: `/api/send-message` (POST, JWT auth).
   - Federation: ActivityPub inboxes/outboxes at `/users/{username}/inbox`.
   - Notifications: Setup webhooks in dashboard for external services; Redis handles internal pub/sub.

7. **Testing**:
   - Use tools like Postman for API testing (with JWT from login endpoint).
   - For federation: Connect to another Sync server or compatible fediverse instance.
   - Notifications: Test push via dashboard simulator.

### Customization

- **Max Users**: Set in `.env` or dashboard; auto-advice uses sysinfo crate in Rust to check specs (e.g., 50 users per GB RAM).
- **Notifications Setup**:
  - Integrate FCM/APNS: Add keys to `.env`; server sends via webhooks (using reqwest).
  - Real-time: Clients subscribe via WebSocket (implemented in Rust API); Redis pub/sub broadcasts events.
- **Auth Flow**: Register/login via API returns JWT; dashboard uses same.

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