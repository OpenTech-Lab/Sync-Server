#!/usr/bin/env bash
# One-time script to bootstrap Let's Encrypt certs on a fresh server.
# Run once after first deploy. After that, the certbot service auto-renews.
#
# Prerequisites:
#   - .env file present with INSTANCE_DOMAIN, ADMIN_DOMAIN, ADMIN_EMAIL set
#   - Ports 80 and 443 open on the server
#   - DNS A records for INSTANCE_DOMAIN and ADMIN_DOMAIN pointing to this server
#
# Usage:
#   bash scripts/init-ssl.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SERVER_DIR="$(dirname "$SCRIPT_DIR")"

# Load .env
if [[ -f "$SERVER_DIR/.env" ]]; then
  set -a; source "$SERVER_DIR/.env"; set +a
else
  echo "ERROR: $SERVER_DIR/.env not found. Copy .env.example and fill in values."
  exit 1
fi

: "${INSTANCE_DOMAIN:?Set INSTANCE_DOMAIN in .env}"
: "${ADMIN_DOMAIN:?Set ADMIN_DOMAIN in .env}"
: "${ADMIN_EMAIL:?Set ADMIN_EMAIL in .env}"

cd "$SERVER_DIR"

COMPOSE_PROJECT="$(basename "$SERVER_DIR")"

echo "→ Domain:        $INSTANCE_DOMAIN"
echo "→ Admin domain:  $ADMIN_DOMAIN"
echo "→ Email:         $ADMIN_EMAIL"

# ── 1. Generate dummy self-signed cert for the IP-block default_server ───────
echo ""
echo "→ Generating dummy cert for IP-block server block..."
mkdir -p nginx/certs
openssl req -x509 -nodes -days 1 -newkey rsa:2048 \
  -keyout nginx/certs/dummy.key \
  -out    nginx/certs/dummy.crt \
  -subj   "/CN=dummy" 2>/dev/null

# ── 2. Start backend services ─────────────────────────────────────────────────
echo "→ Starting postgres, redis, api, dashboard..."
docker compose up -d postgres redis api dashboard

echo "→ Waiting for api to be healthy..."
until docker compose exec -T api wget -qO- http://localhost:8080/ready &>/dev/null; do
  printf '.'
  sleep 3
done
echo " ready."

# ── 3. Start a minimal HTTP-only nginx for the ACME challenge ─────────────────
# Full nginx.conf references letsencrypt certs that don't exist yet → fails.
# Use a bootstrap config that only does HTTP + ACME challenge, no SSL.
BOOTSTRAP_CONF=$(mktemp)
cat > "$BOOTSTRAP_CONF" << NGINXEOF
events { worker_connections 1024; }
http {
    server {
        listen 80;
        server_name $INSTANCE_DOMAIN $ADMIN_DOMAIN;
        location /.well-known/acme-challenge/ { root /var/www/certbot; }
        location / { return 200 "bootstrapping"; }
    }
}
NGINXEOF

docker run -d --name nginx-bootstrap \
  --network "${COMPOSE_PROJECT}_default" \
  -p 80:80 \
  -v "$BOOTSTRAP_CONF":/etc/nginx/nginx.conf:ro \
  -v "${COMPOSE_PROJECT}_certbot_webroot":/var/www/certbot \
  nginx:1.27-alpine

# ── 4. Issue certificate ──────────────────────────────────────────────────────
echo "→ Requesting Let's Encrypt certificate..."
docker run --rm \
  --network "${COMPOSE_PROJECT}_default" \
  -v "${COMPOSE_PROJECT}_letsencrypt":/etc/letsencrypt \
  -v "${COMPOSE_PROJECT}_certbot_webroot":/var/www/certbot \
  certbot/certbot certonly \
    --webroot \
    --webroot-path /var/www/certbot \
    --email "$ADMIN_EMAIL" \
    --agree-tos \
    --no-eff-email \
    -d "$INSTANCE_DOMAIN" \
    -d "$ADMIN_DOMAIN"

# ── 5. Switch to full stack ───────────────────────────────────────────────────
echo "→ Removing bootstrap nginx..."
docker stop nginx-bootstrap && docker rm nginx-bootstrap
rm -f "$BOOTSTRAP_CONF"

echo "→ Starting full stack with SSL..."
docker compose up -d

echo ""
echo "✓ Done! SSL is active."
echo "  https://$INSTANCE_DOMAIN"
echo "  https://$ADMIN_DOMAIN"
echo ""
echo "Certs auto-renew every 12 h via the certbot service."
