#!/usr/bin/env bash
# Local HTTPS bootstrap for development.
# - Uses mkcert when available (trusted local CA)
# - Falls back to openssl self-signed cert
# - Installs certs into the docker volume path nginx already uses:
#   /etc/letsencrypt/live/${INSTANCE_DOMAIN}/(fullchain.pem,privkey.pem)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SERVER_DIR="$(dirname "$SCRIPT_DIR")"

if [[ -f "$SERVER_DIR/.env" ]]; then
  INSTANCE_DOMAIN="$(grep -E '^INSTANCE_DOMAIN=' "$SERVER_DIR/.env" | tail -n1 | cut -d= -f2-)"
  INSTANCE_DOMAIN="${INSTANCE_DOMAIN%\"}"
  INSTANCE_DOMAIN="${INSTANCE_DOMAIN#\"}"
  INSTANCE_DOMAIN="${INSTANCE_DOMAIN%\'}"
  INSTANCE_DOMAIN="${INSTANCE_DOMAIN#\'}"
else
  echo "ERROR: $SERVER_DIR/.env not found. Copy .env.example first."
  exit 1
fi

INSTANCE_DOMAIN="${INSTANCE_DOMAIN:-localhost}"

# Allow only local/private addresses; reject public domain names.
is_local() {
  local d="$1"
  [[ "$d" == "localhost" ]] && return 0
  [[ "$d" =~ ^127\. ]] && return 0
  [[ "$d" =~ ^10\. ]] && return 0
  [[ "$d" =~ ^192\.168\. ]] && return 0
  [[ "$d" =~ ^172\.(1[6-9]|2[0-9]|3[01])\. ]] && return 0
  [[ "$d" == "::1" ]] && return 0
  return 1
}

if ! is_local "$INSTANCE_DOMAIN"; then
  echo "ERROR: init-ssl-dev.sh is intended for local/LAN HTTPS only."
  echo "INSTANCE_DOMAIN='$INSTANCE_DOMAIN' looks like a public domain."
  echo "Use scripts/init-ssl.sh for real domains, or set INSTANCE_DOMAIN to a"
  echo "local address (localhost, 192.168.x.x, 10.x.x.x, …)."
  exit 1
fi

cd "$SERVER_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FULLCHAIN_PATH="$TMP_DIR/fullchain.pem"
PRIVKEY_PATH="$TMP_DIR/privkey.pem"

echo "-> Generating localhost certificate..."
if command -v mkcert >/dev/null 2>&1; then
  mkcert -install >/dev/null 2>&1 || true
  # Include INSTANCE_DOMAIN in SAN so the cert is valid for the LAN IP too.
  mkcert -cert-file "$FULLCHAIN_PATH" -key-file "$PRIVKEY_PATH" "$INSTANCE_DOMAIN" localhost 127.0.0.1 ::1
else
  # Build SAN list: always include localhost entries + INSTANCE_DOMAIN if it's an IP.
  SAN="DNS:localhost,IP:127.0.0.1,IP:::1"
  if [[ "$INSTANCE_DOMAIN" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    SAN="IP:$INSTANCE_DOMAIN,$SAN"
  elif [[ "$INSTANCE_DOMAIN" != "localhost" ]]; then
    SAN="DNS:$INSTANCE_DOMAIN,$SAN"
  fi

  cat > "$TMP_DIR/openssl.cnf" << EOF
[req]
default_bits = 2048
prompt = no
default_md = sha256
x509_extensions = v3_req
distinguished_name = dn

[dn]
CN = $INSTANCE_DOMAIN

[v3_req]
subjectAltName = $SAN
EOF
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
    -keyout "$PRIVKEY_PATH" \
    -out "$FULLCHAIN_PATH" \
    -config "$TMP_DIR/openssl.cnf" >/dev/null 2>&1
fi

mkdir -p nginx/certs
if [[ ! -f nginx/certs/dummy.crt || ! -f nginx/certs/dummy.key ]]; then
  echo "-> Creating default dummy cert for nginx fallback server..."
  openssl req -x509 -nodes -days 1 -newkey rsa:2048 \
    -keyout nginx/certs/dummy.key \
    -out nginx/certs/dummy.crt \
    -subj "/CN=dummy" >/dev/null 2>&1
fi

echo "-> Starting core services to discover compose project..."
docker compose up -d postgres redis api dashboard

API_CONTAINER="$(docker compose ps -q api | head -1)"
if [[ -z "$API_CONTAINER" ]]; then
  echo "ERROR: could not resolve api container id."
  exit 1
fi

COMPOSE_PROJECT="$(docker inspect "$API_CONTAINER" \
  --format '{{index .Config.Labels "com.docker.compose.project"}}')"
LETSENCRYPT_VOL="${COMPOSE_PROJECT}_letsencrypt"

echo "-> Installing localhost cert into docker volume: $LETSENCRYPT_VOL"
docker run --rm \
  -v "$LETSENCRYPT_VOL":/etc/letsencrypt \
  -v "$TMP_DIR":/work:ro \
  alpine sh -lc "
    mkdir -p /etc/letsencrypt/live/$INSTANCE_DOMAIN &&
    cp /work/fullchain.pem /etc/letsencrypt/live/$INSTANCE_DOMAIN/fullchain.pem &&
    cp /work/privkey.pem /etc/letsencrypt/live/$INSTANCE_DOMAIN/privkey.pem
  "

echo "-> Starting full stack..."
docker compose up -d

echo ""
echo "Done. Local HTTPS is available at:"
echo "  https://$INSTANCE_DOMAIN"
echo "  https://$INSTANCE_DOMAIN/login"
echo ""
if command -v mkcert >/dev/null 2>&1; then
  CA_ROOT="$(mkcert -CAROOT)"
  echo "To trust the cert on your phone (one-time):"
  echo "  CA file: $CA_ROOT/rootCA.pem"
  echo "  → Email/AirDrop it to the phone, then:"
  echo "     iOS:     Settings → General → VPN & Device Management → trust the CA"
  echo "     Android: Settings → Security → Install certificate → CA certificate"
else
  echo "Note: openssl fallback cert is self-signed; browsers and apps will warn."
  echo "Install mkcert for a properly trusted local cert."
fi
