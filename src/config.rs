use anyhow::{Context, Result};
use sysinfo::System;

#[derive(Clone, Debug)]
pub struct Config {
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
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Config {
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL must be set")?,
            redis_url: std::env::var("REDIS_URL")
                .context("REDIS_URL must be set")?,
            jwt_secret: std::env::var("JWT_SECRET")
                .context("JWT_SECRET must be set")?,
            jwt_access_expiry_secs: std::env::var("JWT_ACCESS_EXPIRY_SECS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .context("JWT_ACCESS_EXPIRY_SECS must be a valid number")?,
            jwt_refresh_expiry_secs: std::env::var("JWT_REFRESH_EXPIRY_SECS")
                .unwrap_or_else(|_| "2592000".to_string())
                .parse()
                .context("JWT_REFRESH_EXPIRY_SECS must be a valid number")?,
            server_host: std::env::var("SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("SERVER_PORT must be a valid port number")?,
            instance_name: std::env::var("INSTANCE_NAME")
                .unwrap_or_else(|_| "sync-planet".to_string()),
            instance_domain: std::env::var("INSTANCE_DOMAIN")
                .unwrap_or_else(|_| "localhost".to_string()),
            admin_email: std::env::var("ADMIN_EMAIL")
                .context("ADMIN_EMAIL must be set")?,
            max_users: std::env::var("MAX_USERS")
                .ok()
                .and_then(|v| v.parse().ok()),
        })
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
