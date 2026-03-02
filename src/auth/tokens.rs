use anyhow::{Context, Result};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use ring::rand::SecureRandom;
use ring::{digest, rand};

use super::claims::Claims;

/// Sign and return a JWT access token.
pub fn issue_access_token(claims: &Claims, secret: &str) -> Result<String> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("Failed to encode JWT")
}

/// Verify a JWT access token and return its decoded claims.
pub fn verify_access_token(token: &str, secret: &str) -> Result<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .context("Failed to decode JWT")
}

/// Generate a 32-byte cryptographically random refresh token.
///
/// Returns `(raw_base64, sha256_hex_hash)`.
/// Store the hash in the DB; send the raw token to the client.
pub fn generate_refresh_token() -> (String, String) {
    let rng = rand::SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes).expect("RNG failed");
    let raw = hex::encode(bytes);
    let hash = hash_token(&raw);
    (raw, hash)
}

/// SHA-256 hash of a token string, returned as a lowercase hex string.
pub fn hash_token(raw: &str) -> String {
    let digest = digest::digest(&digest::SHA256, raw.as_bytes());
    hex::encode(digest.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::claims::Claims;
    use uuid::Uuid;

    fn make_claims(exp_offset_secs: i64) -> Claims {
        let now = chrono::Utc::now().timestamp();
        Claims::new(Uuid::new_v4(), "user".into(), now, now + exp_offset_secs)
    }

    #[test]
    fn issue_verify_roundtrip() {
        let secret = "test-secret-key-for-unit-tests-only";
        let claims = make_claims(3600);
        let token = issue_access_token(&claims, secret).unwrap();
        let decoded = verify_access_token(&token, secret).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.role, claims.role);
    }

    #[test]
    fn expired_token_rejected() {
        let secret = "test-secret-key-for-unit-tests-only";
        let claims = make_claims(-3600); // expired 1 hour ago, well past any leeway
        let token = issue_access_token(&claims, secret).unwrap();
        assert!(verify_access_token(&token, secret).is_err());
    }

    #[test]
    fn tampered_token_rejected() {
        let secret = "test-secret-key-for-unit-tests-only";
        let claims = make_claims(3600);
        let mut token = issue_access_token(&claims, secret).unwrap();
        // Corrupt the signature portion
        token.push_str("TAMPERED");
        assert!(verify_access_token(&token, secret).is_err());
    }

    #[test]
    fn hash_token_is_deterministic() {
        let h1 = hash_token("some-refresh-token");
        let h2 = hash_token("some-refresh-token");
        assert_eq!(h1, h2);
    }

    #[test]
    fn generate_refresh_token_produces_unique_pairs() {
        let (raw1, hash1) = generate_refresh_token();
        let (raw2, hash2) = generate_refresh_token();
        assert_ne!(raw1, raw2);
        assert_ne!(hash1, hash2);
        // Hash is consistent with raw
        assert_eq!(hash_token(&raw1), hash1);
    }
}
