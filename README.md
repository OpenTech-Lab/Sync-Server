# Sync Server

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub Repo](https://img.shields.io/badge/GitHub-sync-blue?logo=github)](https://github.com/OpenTech-Lab/sync-server)

Sync Server is the backend component of the open-source, privacy-focused Sync chat application. It provides a decentralized, federated messaging platform inspired by Mastodon and Matrix, using ActivityPub for interoperability between servers (called "planets"). The server handles secure API endpoints for message sending, user management, and optional encrypted data backups. Emphasizing security and user control, chat data is primarily stored locally on client devices, with server-side storage only for opted-in, encrypted backups (similar to Signal).

This repo focuses on the server-side: Rust-based API, Next.js dashboard for administration, and easy deployment via Docker Compose. For the mobile client, see the companion repo: [sync-mobile](https://github.com/OpenTech-Lab/sync-mobile).

![Sync](docs/images/sync-banner.png)

## How to use
[FINISH THIS BLOCK]

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