#!/usr/bin/env bash
# Generate a self-signed TLS certificate for local development.
# For production, replace nginx/certs/ with real certs from Let's Encrypt.
set -euo pipefail

CERT_DIR="$(dirname "$0")/../nginx/certs"
mkdir -p "$CERT_DIR"

openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout "$CERT_DIR/server.key" \
  -out    "$CERT_DIR/server.crt" \
  -subj   "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

echo "Dev certs written to $CERT_DIR/"
