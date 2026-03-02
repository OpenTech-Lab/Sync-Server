use actix_web::web;
use anyhow::Result;

const BCRYPT_COST: u32 = 12;

/// Hash a plaintext password using bcrypt (cost 12).
///
/// Run in a blocking thread pool via `web::block` so it doesn't stall the
/// async runtime.
pub async fn hash_password(password: String) -> Result<String> {
    web::block(move || bcrypt::hash(&password, BCRYPT_COST))
        .await
        .map_err(|e| anyhow::anyhow!("blocking error: {}", e))?
        .map_err(|e| anyhow::anyhow!("bcrypt hash error: {}", e))
}

/// Verify a plaintext password against a stored bcrypt hash.
pub async fn verify_password(password: String, hash: String) -> Result<bool> {
    web::block(move || bcrypt::verify(&password, &hash))
        .await
        .map_err(|e| anyhow::anyhow!("blocking error: {}", e))?
        .map_err(|e| anyhow::anyhow!("bcrypt verify error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn hash_verify_roundtrip() {
        let pw = "correct-horse-battery-staple".to_string();
        let hash = hash_password(pw.clone()).await.unwrap();
        assert!(verify_password(pw, hash).await.unwrap());
    }

    #[actix_web::test]
    async fn wrong_password_rejected() {
        let hash = hash_password("correct".to_string()).await.unwrap();
        assert!(!verify_password("wrong".to_string(), hash).await.unwrap());
    }
}
