## Sync Admin Dashboard

Operational Next.js dashboard for Sync server administration.

### Features
- Admin login/session flow backed by `sync-server` auth (`/auth/login`, `/auth/me`, `/auth/logout`)
- Dashboard overview for system/user/federation queue health
- User management (list/search + suspend/activate)
- Config management (`max_users`, notification webhook URL)
- Audit log viewer for admin actions

### Environment

Create `.env.local` in this directory as needed:

```bash
# Base URL for the Rust API used by dashboard route handlers
SYNC_SERVER_URL=http://localhost:8080

# Optional override for local e2e
# DASHBOARD_BASE_URL=http://localhost:3000
```

### Development

Install dependencies and run:

```bash
npm install
npm run dev
```

Open `http://localhost:3000`.

### Quality checks

```bash
npm run lint
npm run build
npm run test
npm run test:e2e
```

### Notes
- Admin APIs are exposed by the Rust service under `/api/admin/*`.
- Session cookies are `httpOnly` and `SameSite=Strict`.
- Request hardening and security headers are enforced in `proxy.ts`.

To learn more about Next.js, take a look at the following resources:

- [Next.js Documentation](https://nextjs.org/docs) - learn about Next.js features and API.
- [Learn Next.js](https://nextjs.org/learn) - an interactive Next.js tutorial.

You can check out [the Next.js GitHub repository](https://github.com/vercel/next.js) - your feedback and contributions are welcome!

## Deploy on Vercel

The easiest way to deploy your Next.js app is to use the [Vercel Platform](https://vercel.com/new?utm_medium=default-template&filter=next.js&utm_source=create-next-app&utm_campaign=create-next-app-readme) from the creators of Next.js.

Check out our [Next.js deployment documentation](https://nextjs.org/docs/app/building-your-application/deploying) for more details.
