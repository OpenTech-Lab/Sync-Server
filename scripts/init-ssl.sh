#!/usr/bin/env bash
# One-time script to bootstrap Let's Encrypt certs on a fresh server.
# Run once after first deploy. After that, the certbot service auto-renews.
#
# Prerequisites:
#   - .env file present with INSTANCE_DOMAIN, ADMIN_DOMAIN, ADMIN_EMAIL set
#   - Ports 80 and 443 open on the server
#   - DNS A/AAAA records for:
#       - INSTANCE_DOMAIN
#       - push.INSTANCE_DOMAIN
#     both pointing to this server
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
: "${ADMIN_EMAIL:?Set ADMIN_EMAIL in .env}"
PUSH_DOMAIN="push.${INSTANCE_DOMAIN}"

cd "$SERVER_DIR"

echo "→ Domain:  $INSTANCE_DOMAIN"
echo "→ Push:    $PUSH_DOMAIN"
echo "→ Email:   $ADMIN_EMAIL"

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

# ── 3. Discover the actual Docker network and volume names ────────────────────
# Don't guess the compose project name — inspect the running api container.
API_CONTAINER=$(docker compose ps -q api | head -1)

# Use the compose project label to construct the network name reliably.
# (Iterating NetworkSettings.Networks concatenates all networks without
# a separator, which breaks if the container is on multiple networks.)
COMPOSE_PROJECT=$(docker inspect "$API_CONTAINER" \
  --format '{{index .Config.Labels "com.docker.compose.project"}}')
NETWORK_NAME="${COMPOSE_PROJECT}_default"

LETSENCRYPT_VOL="${COMPOSE_PROJECT}_letsencrypt"
CERTBOT_WEBROOT_VOL="${COMPOSE_PROJECT}_certbot_webroot"

echo "→ Docker network:       $NETWORK_NAME"
echo "→ letsencrypt volume:   $LETSENCRYPT_VOL"
echo "→ certbot_webroot vol:  $CERTBOT_WEBROOT_VOL"

# ── 4. Free port 80 before bootstrap nginx ────────────────────────────────────
echo "→ Stopping compose nginx (if running) to free port 80..."
docker compose stop nginx 2>/dev/null || true
# Clean up any leftover bootstrap container from a previous failed run
docker rm -f nginx-bootstrap 2>/dev/null || true

# Hard-verify port 80 is actually free; bail with a clear message if not.
if ss -tlnp 2>/dev/null | grep -q ':80 ' || \
   netstat -tlnp 2>/dev/null | grep -q ':80 '; then
  echo ""
  echo "ERROR: Port 80 is still in use. Run 'docker compose down' first, then re-run this script."
  exit 1
fi

# ── 4b. Start a minimal HTTP-only nginx for the ACME challenge ────────────────
BOOTSTRAP_CONF=$(mktemp)
cat > "$BOOTSTRAP_CONF" << NGINXEOF
events { worker_connections 1024; }
http {
    server {
        listen 80;
        server_name $INSTANCE_DOMAIN $PUSH_DOMAIN;
        location /.well-known/acme-challenge/ { root /var/www/certbot; }
        location / { return 200 "bootstrapping"; }
    }
}
NGINXEOF

docker run -d --name nginx-bootstrap \
  --network "$NETWORK_NAME" \
  -p 80:80 \
  -v "$BOOTSTRAP_CONF":/etc/nginx/nginx.conf:ro \
  -v "$CERTBOT_WEBROOT_VOL":/var/www/certbot \
  nginx:1.27-alpine

# ── 5. Issue certificate ──────────────────────────────────────────────────────
echo "→ Requesting Let's Encrypt certificate..."
docker run --rm \
  --network "$NETWORK_NAME" \
  -v "$LETSENCRYPT_VOL":/etc/letsencrypt \
  -v "$CERTBOT_WEBROOT_VOL":/var/www/certbot \
  certbot/certbot certonly \
    --webroot \
    --webroot-path /var/www/certbot \
    --email "$ADMIN_EMAIL" \
    --agree-tos \
    --no-eff-email \
    -d "$INSTANCE_DOMAIN" \
    -d "$PUSH_DOMAIN"

# ── 6. Switch to full stack ───────────────────────────────────────────────────
echo "→ Removing bootstrap nginx..."
docker stop nginx-bootstrap && docker rm nginx-bootstrap
rm -f "$BOOTSTRAP_CONF"

echo "→ Starting full stack with SSL..."
docker compose up -d

echo ""
echo "✓ Done! SSL is active."
echo "  https://$INSTANCE_DOMAIN"
echo "  https://$INSTANCE_DOMAIN/login"
echo "  https://$PUSH_DOMAIN"
echo ""
echo "Certs auto-renew every 12 h via the certbot service."
