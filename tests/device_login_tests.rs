/// Integration tests for `POST /auth/device-login` (challenge/proof and the
/// legacy body) and the QR-login poll (`POST` and the legacy `GET`).
///
/// These tests require a running server, Redis and a real database.
/// Set `TEST_BASE_URL` (e.g. `http://localhost:8080`) to run them.
/// If the env var is absent the tests are skipped.
///
/// The legacy body is gated by the server's `DEVICE_LOGIN_REQUIRE_CHALLENGE`
/// flag (default off). Run once per mode: with the flag on, also set
/// `TEST_DEVICE_LOGIN_REQUIRE_CHALLENGE=true` so the expectation matches.
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use reqwest::{Client, Response};
use ring::{agreement, hmac, rand::SystemRandom};
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn challenge_required() -> bool {
    matches!(
        std::env::var("TEST_DEVICE_LOGIN_REQUIRE_CHALLENGE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// POST JSON, retrying while the per-IP `/auth` rate limiter says 429.
async fn post(client: &Client, url: &str, body: &Value, ip: &str) -> Response {
    post_as(client, url, body, ip, None).await
}

async fn post_as(
    client: &Client,
    url: &str,
    body: &Value,
    ip: &str,
    bearer: Option<&str>,
) -> Response {
    for _ in 0..20 {
        let mut req = client
            .post(url)
            .header("X-Real-IP", ip)
            .header("User-Agent", "Sync/1.3.2 (integration-test)")
            .json(body);
        if let Some(token) = bearer {
            req = req.bearer_auth(token);
        }
        let res = req.send().await.expect("request failed");
        if res.status() != 429 {
            return res;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    panic!("still rate limited: {url}");
}

/// GET, retrying while the per-IP `/auth` rate limiter says 429.
async fn get(client: &Client, url: &str) -> Response {
    for _ in 0..20 {
        let res = client.get(url).send().await.expect("request failed");
        if res.status() != 429 {
            return res;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    panic!("still rate limited: {url}");
}

/// Registration is capped per client IP per hour; give each device its own.
fn fresh_ip() -> String {
    let octets = Uuid::new_v4().into_bytes();
    format!("10.{}.{}.{}", octets[0], octets[1], octets[2])
}

/// The device side of the protocol, as the mobile client implements it.
struct Device {
    private: agreement::EphemeralPrivateKey,
    public_b64: String,
    ip: String,
}

impl Device {
    fn new() -> Self {
        let private =
            agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
                .unwrap();
        let public_b64 = BASE64_STANDARD.encode(private.compute_public_key().unwrap());
        Self {
            private,
            public_b64,
            ip: fresh_ip(),
        }
    }

    /// proof = base64(HMAC-SHA256(X25519(device_priv, server_pub),
    ///                            "device-login:<challenge_id>:<device_pub_b64>"))
    fn proof(self, challenge_id: &str, server_pubkey_b64: &str) -> String {
        let server_public = BASE64_STANDARD.decode(server_pubkey_b64).unwrap();
        let shared = agreement::agree_ephemeral(
            self.private,
            &agreement::UnparsedPublicKey::new(&agreement::X25519, server_public),
            |material| material.to_vec(),
        )
        .unwrap();
        let tag = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, &shared),
            format!("device-login:{challenge_id}:{}", self.public_b64).as_bytes(),
        );
        BASE64_STANDARD.encode(tag.as_ref())
    }
}

/// Requests a challenge for `device` and returns `(challenge_id, proof)`.
async fn challenge_and_proof(client: &Client, base: &str, device: Device) -> (String, String) {
    let res = post(
        client,
        &format!("{base}/auth/device-login/challenge"),
        &json!({ "device_auth_pubkey": device.public_b64 }),
        &device.ip,
    )
    .await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["cache-control"], "no-store");
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["expires_in"], 60);
    let challenge_id = body["challenge_id"].as_str().unwrap().to_owned();
    let proof = device.proof(
        &challenge_id,
        body["server_ephemeral_pubkey"].as_str().unwrap(),
    );
    (challenge_id, proof)
}

/// A device that answers a challenge gets tokens; replaying that exact login is refused.
#[tokio::test]
async fn device_login_with_a_proof_succeeds_once_and_cannot_be_replayed() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let device = Device::new();
    let (pubkey, ip) = (device.public_b64.clone(), device.ip.clone());
    let (challenge_id, proof) = challenge_and_proof(&client, &base, device).await;
    let body = json!({
        "device_auth_pubkey": pubkey,
        "challenge_id": challenge_id,
        "proof": proof,
    });

    let res = post(&client, &format!("{base}/auth/device-login"), &body, &ip).await;
    assert_eq!(res.status(), 200);
    let tokens: Value = res.json().await.unwrap();
    assert!(tokens["access_token"]
        .as_str()
        .is_some_and(|t| !t.is_empty()));
    assert!(tokens["refresh_token"]
        .as_str()
        .is_some_and(|t| !t.is_empty()));

    let replay = post(&client, &format!("{base}/auth/device-login"), &body, &ip).await;
    assert_eq!(replay.status(), 401, "challenge must be single-use");
}

/// A wrong proof is refused and burns the challenge.
#[tokio::test]
async fn device_login_with_a_wrong_proof_is_unauthorized() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let device = Device::new();
    let (pubkey, ip) = (device.public_b64.clone(), device.ip.clone());
    let (challenge_id, proof) = challenge_and_proof(&client, &base, device).await;

    let wrong = json!({
        "device_auth_pubkey": pubkey,
        "challenge_id": challenge_id,
        "proof": BASE64_STANDARD.encode([0u8; 32]),
    });
    let res = post(&client, &format!("{base}/auth/device-login"), &wrong, &ip).await;
    assert_eq!(res.status(), 401);

    let right = json!({
        "device_auth_pubkey": pubkey,
        "challenge_id": challenge_id,
        "proof": proof,
    });
    let res = post(&client, &format!("{base}/auth/device-login"), &right, &ip).await;
    assert_eq!(
        res.status(),
        401,
        "a failed attempt must burn the challenge"
    );
}

/// Sending only one of `challenge_id` / `proof` is never treated as a legacy login.
#[tokio::test]
async fn device_login_with_only_one_of_challenge_id_and_proof_is_rejected() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let device = Device::new();
    for extra in [
        json!({ "challenge_id": "ab".repeat(32) }),
        json!({ "proof": "AAAA" }),
    ] {
        let mut body = json!({ "device_auth_pubkey": device.public_b64 });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let res = post(
            &client,
            &format!("{base}/auth/device-login"),
            &body,
            &device.ip,
        )
        .await;
        assert_eq!(res.status(), 400);
    }
}

/// The body installed apps <= 1.3.2 send: accepted while the flag is off, refused when on.
#[tokio::test]
async fn legacy_device_login_body_follows_the_transition_flag() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let device = Device::new();
    let body = json!({ "device_auth_pubkey": device.public_b64, "accepted_terms_version": 1 });

    let res = post(
        &client,
        &format!("{base}/auth/device-login"),
        &body,
        &device.ip,
    )
    .await;
    if challenge_required() {
        assert_eq!(res.status(), 400);
    } else {
        assert_eq!(res.status(), 200);
        let tokens: Value = res.json().await.unwrap();
        assert!(tokens["access_token"]
            .as_str()
            .is_some_and(|t| !t.is_empty()));
    }
}

/// Both poll forms read the same session; the secret travels in the body for POST
/// and in the query string only for the legacy GET.
#[tokio::test]
async fn qr_login_session_can_be_polled_with_post_and_with_the_legacy_get() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let ip = fresh_ip();

    // An approver: any registered device account will do.
    let approver = Device::new();
    let approver_ip = approver.ip.clone();
    let approver_tokens: Value = post(
        &client,
        &format!("{base}/auth/device-login"),
        &if challenge_required() {
            let pubkey = approver.public_b64.clone();
            let (challenge_id, proof) = challenge_and_proof(&client, &base, approver).await;
            json!({ "device_auth_pubkey": pubkey, "challenge_id": challenge_id, "proof": proof })
        } else {
            json!({ "device_auth_pubkey": approver.public_b64 })
        },
        &approver_ip,
    )
    .await
    .json()
    .await
    .unwrap();
    let approver_token = approver_tokens["access_token"].as_str().unwrap().to_owned();

    for use_post in [true, false] {
        let created: Value = post(
            &client,
            &format!("{base}/auth/qr-login/session"),
            &json!({}),
            &ip,
        )
        .await
        .json()
        .await
        .unwrap();
        let session_id = created["session_id"].as_str().unwrap().to_owned();
        let payload: Value = serde_json::from_str(created["qr_payload"].as_str().unwrap()).unwrap();
        let secret = payload["secret"].as_str().unwrap().to_owned();
        let url = format!("{base}/auth/qr-login/session/{session_id}");

        let poll = |secret: String| {
            let (client, url, ip) = (client.clone(), url.clone(), ip.clone());
            async move {
                if use_post {
                    post(&client, &url, &json!({ "secret": secret }), &ip).await
                } else {
                    get(&client, &format!("{url}?secret={secret}")).await
                }
            }
        };

        let pending: Value = poll(secret.clone()).await.json().await.unwrap();
        assert_eq!(pending["status"], "pending", "post={use_post}");
        assert_eq!(poll("wrong".into()).await.status(), 401, "post={use_post}");

        let approve = post_as(
            &client,
            &format!("{base}/auth/qr-login/approve"),
            &json!({ "session_id": session_id, "secret": secret }),
            &ip,
            Some(&approver_token),
        )
        .await;
        assert_eq!(approve.status(), 200, "post={use_post}");

        let approved = poll(secret.clone()).await;
        if use_post {
            assert_eq!(approved.headers()["cache-control"], "no-store");
        }
        let approved: Value = approved.json().await.unwrap();
        assert_eq!(approved["status"], "approved", "post={use_post}");
        assert!(approved["access_token"]
            .as_str()
            .is_some_and(|t| !t.is_empty()));
    }
}
