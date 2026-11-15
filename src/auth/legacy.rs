//! In-process counters for the deprecated auth paths kept for installed apps
//! (<= 1.3.2). Shown in `GET /api/admin/overview` so an operator can watch them
//! fall to zero before turning the matching enforcement flag on.

use std::sync::atomic::{AtomicU64, Ordering};

/// `POST /auth/device-login` bodies accepted without a challenge/proof.
static DEVICE_LOGINS: AtomicU64 = AtomicU64::new(0);
/// `GET /auth/qr-login/session/{id}?secret=` polls served.
static QR_POLLS: AtomicU64 = AtomicU64::new(0);

pub fn record_device_login() {
    DEVICE_LOGINS.fetch_add(1, Ordering::Relaxed);
}

pub fn device_login_count() -> u64 {
    DEVICE_LOGINS.load(Ordering::Relaxed)
}

pub fn record_qr_poll() {
    QR_POLLS.fetch_add(1, Ordering::Relaxed);
}

pub fn qr_poll_count() -> u64 {
    QR_POLLS.load(Ordering::Relaxed)
}
