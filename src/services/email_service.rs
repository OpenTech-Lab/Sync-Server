use serde::Serialize;

use crate::errors::AppError;

#[derive(Serialize)]
struct ResendRequest<'a> {
    from: &'a str,
    to: [&'a str; 1],
    subject: &'a str,
    html: String,
}

/// Send a password-reset email via the Resend REST API.
///
/// If `api_key` is `None` the call is a no-op (useful for local dev without
/// a real Resend account configured).
pub async fn send_password_reset(
    api_key: Option<&str>,
    from: &str,
    to: &str,
    reset_url: &str,
) -> Result<(), AppError> {
    let Some(key) = api_key else {
        tracing::warn!(
            recipient = %to,
            "RESEND_API_KEY not set — password-reset email skipped"
        );
        return Ok(());
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<body style="font-family:sans-serif;max-width:480px;margin:40px auto;color:#111">
  <h2>Reset your Sync password</h2>
  <p>Click the button below to choose a new password. The link expires in 15 minutes.</p>
  <p>
    <a href="{url}" style="display:inline-block;padding:12px 24px;background:#4F46E5;color:#fff;border-radius:8px;text-decoration:none;font-weight:600">
      Reset password
    </a>
  </p>
  <p style="color:#666;font-size:13px">
    If you did not request a password reset you can safely ignore this email.
  </p>
</body>
</html>"#,
        url = reset_url
    );

    let body = ResendRequest {
        from,
        to: [to],
        subject: "Reset your Sync password",
        html,
    };

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.resend.com/emails")
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Email send failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        tracing::error!(%status, %text, "Resend API error");
        return Err(AppError::Internal(anyhow::anyhow!(
            "Email provider returned {}",
            status
        )));
    }

    tracing::info!(recipient = %to, "Password-reset email sent");
    Ok(())
}
