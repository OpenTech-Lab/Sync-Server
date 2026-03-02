use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT claims payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — user UUID as string.
    pub sub: String,
    /// Role: "user", "admin", or "moderator".
    pub role: String,
    /// Issued-at (Unix seconds).
    pub iat: i64,
    /// Expiry (Unix seconds).
    pub exp: i64,
}

impl Claims {
    pub fn new(user_id: Uuid, role: String, iat: i64, exp: i64) -> Self {
        Claims {
            sub: user_id.to_string(),
            role,
            iat,
            exp,
        }
    }

    /// Parse `sub` back to a `Uuid`.
    pub fn user_id(&self) -> Result<Uuid> {
        Uuid::parse_str(&self.sub).map_err(|e| anyhow::anyhow!("Invalid user_id in token: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_id_roundtrip() {
        let id = Uuid::new_v4();
        let claims = Claims::new(id, "user".into(), 0, 9999999999);
        assert_eq!(claims.user_id().unwrap(), id);
    }

    #[test]
    fn user_id_invalid_sub_errors() {
        let claims = Claims {
            sub: "not-a-uuid".into(),
            role: "user".into(),
            iat: 0,
            exp: 0,
        };
        assert!(claims.user_id().is_err());
    }
}
