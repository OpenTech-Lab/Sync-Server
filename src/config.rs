use anyhow::{Context, Result};
use sysinfo::System;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Config {
    pub app_env: String,
    pub enforce_https: bool,
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub jwt_access_expiry_secs: u64,
    pub jwt_refresh_expiry_secs: u64,
    pub server_host: String,
    pub server_port: u16,
    pub instance_name: String,
    pub instance_domain: String,
    pub admin_email: String,
    pub max_users: Option<u32>,
    pub federation_denylist: Vec<String>,
    pub federation_signature_window_secs: i64,
    pub federation_max_delivery_attempts: i32,
    pub federation_key_cache_ttl_secs: i64,
    pub federation_inbox_rps: u64,
    pub federation_inbox_burst: u32,
    /// Resend API key. `None` → email sending is skipped with a warning.
    pub resend_api_key: Option<String>,
    /// From address used in password-reset emails.
    pub resend_from_email: String,
    /// Apple Developer Team ID for APNs token-based auth.
    pub apns_team_id: Option<String>,
    /// Apple Key ID for APNs token-based auth.
    pub apns_key_id: Option<String>,
    /// iOS app bundle identifier used as APNs topic.
    pub apns_bundle_id: Option<String>,
    /// APNs private key (.p8) contents (raw PEM, \n-escaped PEM, or base64-encoded PEM).
    pub apns_private_key_p8: Option<String>,
    /// Use APNs sandbox endpoint (development builds).
    pub apns_use_sandbox: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Config {
            app_env: std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string()),
            enforce_https: std::env::var("ENFORCE_HTTPS")
                .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false),
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
            redis_url: std::env::var("REDIS_URL").context("REDIS_URL must be set")?,
            jwt_secret: std::env::var("JWT_SECRET").context("JWT_SECRET must be set")?,
            jwt_access_expiry_secs: std::env::var("JWT_ACCESS_EXPIRY_SECS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .context("JWT_ACCESS_EXPIRY_SECS must be a valid number")?,
            jwt_refresh_expiry_secs: std::env::var("JWT_REFRESH_EXPIRY_SECS")
                .unwrap_or_else(|_| "2592000".to_string())
                .parse()
                .context("JWT_REFRESH_EXPIRY_SECS must be a valid number")?,
            server_host: std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("SERVER_PORT must be a valid port number")?,
            instance_name: std::env::var("INSTANCE_NAME")
                .unwrap_or_else(|_| "sync-planet".to_string()),
            instance_domain: std::env::var("INSTANCE_DOMAIN")
                .unwrap_or_else(|_| "localhost".to_string()),
            admin_email: std::env::var("ADMIN_EMAIL").context("ADMIN_EMAIL must be set")?,
            max_users: std::env::var("MAX_USERS").ok().and_then(|v| v.parse().ok()),
            federation_denylist: std::env::var("FEDERATION_DENYLIST")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
                .collect(),
            federation_signature_window_secs: std::env::var("FEDERATION_SIGNATURE_WINDOW_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("FEDERATION_SIGNATURE_WINDOW_SECS must be a valid number")?,
            federation_max_delivery_attempts: std::env::var("FEDERATION_MAX_DELIVERY_ATTEMPTS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .context("FEDERATION_MAX_DELIVERY_ATTEMPTS must be a valid number")?,
            federation_key_cache_ttl_secs: std::env::var("FEDERATION_KEY_CACHE_TTL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("FEDERATION_KEY_CACHE_TTL_SECS must be a valid number")?,
            federation_inbox_rps: std::env::var("FEDERATION_INBOX_RPS")
                .unwrap_or_else(|_| "20".to_string())
                .parse()
                .context("FEDERATION_INBOX_RPS must be a valid number")?,
            federation_inbox_burst: std::env::var("FEDERATION_INBOX_BURST")
                .unwrap_or_else(|_| "40".to_string())
                .parse()
                .context("FEDERATION_INBOX_BURST must be a valid number")?,
            resend_api_key: std::env::var("RESEND_API_KEY").ok(),
            resend_from_email: std::env::var("RESEND_FROM_EMAIL")
                .unwrap_or_else(|_| "noreply@localhost".to_string()),
            apns_team_id: std::env::var("APNS_TEAM_ID")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            apns_key_id: std::env::var("APNS_KEY_ID")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            apns_bundle_id: std::env::var("APNS_BUNDLE_ID")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            apns_private_key_p8: std::env::var("APNS_PRIVATE_KEY_P8")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            apns_use_sandbox: std::env::var("APNS_USE_SANDBOX")
                .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false),
        })
    }

    pub fn validate_security_defaults(&self) -> Result<()> {
        if self.app_env.eq_ignore_ascii_case("production") {
            if !self.enforce_https {
                return Err(anyhow::anyhow!(
                    "ENFORCE_HTTPS=true is required when APP_ENV=production"
                ));
            }

            if self.jwt_secret.len() < 32 || self.jwt_secret.contains("change-me") {
                return Err(anyhow::anyhow!(
                    "JWT_SECRET must be a strong non-default value in production"
                ));
            }

            if self.instance_domain.eq_ignore_ascii_case("localhost") {
                return Err(anyhow::anyhow!(
                    "INSTANCE_DOMAIN must be a public domain in production"
                ));
            }
        }

        Ok(())
    }

    /// Log host hardware specs and the suggested user cap based on available resources.
    pub fn log_host_spec(&self) {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu_count = sys.cpus().len();
        let total_mem_mb = sys.total_memory() / 1024 / 1024;
        let suggested_max = suggest_max_users(cpu_count, total_mem_mb);

        tracing::info!(
            cpu_count,
            total_memory_mb = total_mem_mb,
            suggested_max_users = suggested_max,
            configured_max_users = ?self.max_users,
            "Host specification"
        );

        if let Some(max) = self.max_users {
            if max > suggested_max {
                tracing::warn!(
                    configured = max,
                    suggested = suggested_max,
                    "MAX_USERS exceeds suggested limit for this host's resources"
                );
            }
        }
    }
}

/// Heuristic: ~100 concurrent sessions per CPU core, bounded by available memory (50 MB/user).
fn suggest_max_users(cpu_count: usize, total_mem_mb: u64) -> u32 {
    let by_cpu = (cpu_count as u32).saturating_mul(100);
    let by_mem = (total_mem_mb / 50) as u32;
    by_cpu.min(by_mem).max(10)
}
