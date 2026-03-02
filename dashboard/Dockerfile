# ──────────────────────────────────────────────
# Stage 1: Install dependencies and build
# ──────────────────────────────────────────────
FROM node:22-alpine AS builder

WORKDIR /app

COPY package.json package-lock.json* ./
RUN npm ci

COPY . .
RUN npm run build

# ──────────────────────────────────────────────
# Stage 2: Minimal production runner
# ──────────────────────────────────────────────
FROM node:22-alpine AS runner

ENV NODE_ENV=production
ENV PORT=3000

RUN addgroup -S nextjs && adduser -S nextjs -G nextjs

WORKDIR /app

# Copy standalone output + static assets
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
COPY --from=builder /app/public ./public

USER nextjs

EXPOSE 3000

CMD ["node", "server.js"]
