use std::sync::atomic::{AtomicU64, Ordering};

pub mod broker;
pub mod session;

/// Handshakes authenticated through the deprecated `GET /ws?token=<JWT>` form.
static LEGACY_TOKEN_AUTHS: AtomicU64 = AtomicU64::new(0);

pub fn record_legacy_token_auth() {
    LEGACY_TOKEN_AUTHS.fetch_add(1, Ordering::Relaxed);
}

pub fn legacy_token_auth_count() -> u64 {
    LEGACY_TOKEN_AUTHS.load(Ordering::Relaxed)
}
