use actix_web::{web, App, HttpResponse, HttpServer};
use base64::Engine;
use reqwest::Client;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn unique_username() -> String {
    format!("fed_{}", Uuid::new_v4().simple())
}

fn unique_email() -> String {
    format!("{}@example.com", unique_username())
}

fn encode_raw_pem(label: &str, bytes: &[u8]) -> String {
    let body = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----")
}

async fn register_user(client: &Client, base: &str) -> (String, String, String) {
    let username = unique_username();
    let email = unique_email();
    let password = "test_pass_secure!".to_string();

    let res = client
        .post(format!("{base}/auth/register"))
        .json(&json!({
            "username": username,
            "email": email,
            "password": password,
        }))
        .send()
        .await
        .expect("register request failed");

    assert!(
        res.status().is_success(),
        "register failed: {}",
        res.status()
    );
    (username, email, password)
}

async fn login(client: &Client, base: &str, email: &str, password: &str) -> String {
    let res: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login request failed")
        .json()
        .await
        .expect("login json failed");

    res["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_string()
}

#[tokio::test]
async fn webfinger_returns_actor_link_for_local_user() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (username, _, _) = register_user(&client, &base).await;
    let domain = std::env::var("INSTANCE_DOMAIN").unwrap_or_else(|_| "localhost".into());

    let res = client
        .get(format!(
            "{base}/.well-known/webfinger?resource=acct:{username}@{domain}"
        ))
        .send()
        .await
        .expect("webfinger request failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("invalid json");
    assert_eq!(body["subject"], format!("acct:{username}@{domain}"));
    assert_eq!(body["links"][0]["type"], "application/activity+json");
}

#[tokio::test]
async fn actor_profile_contains_inbox_and_public_key() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (username, _, _) = register_user(&client, &base).await;

    let res = client
        .get(format!("{base}/users/{username}"))
        .send()
        .await
        .expect("actor request failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("invalid json");
    assert_eq!(body["type"], "Person");
    assert!(body["inbox"]
        .as_str()
        .unwrap_or_default()
        .ends_with("/inbox"));
    assert!(body["publicKey"]["publicKeyPem"].as_str().is_some());
}

#[tokio::test]
async fn inbox_rejects_unsigned_requests() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (username, _, _) = register_user(&client, &base).await;

    let res = client
        .post(format!("{base}/users/{username}/inbox"))
        .json(&json!({
            "id": format!("https://remote.example/activities/{}", Uuid::new_v4()),
            "type": "Create",
            "actor": "https://remote.example/users/alice"
        }))
        .send()
        .await
        .expect("inbox request failed");

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn inbox_signed_duplicate_activity_returns_duplicate_on_second_delivery() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (recipient_username, _, _) = register_user(&client, &base).await;

    let rng = SystemRandom::new();
    let generated = Ed25519KeyPair::generate_pkcs8(&rng).expect("keygen failed");
    let pair = Ed25519KeyPair::from_pkcs8(generated.as_ref()).expect("pair parse failed");
    let public_pem = encode_raw_pem("PUBLIC KEY", pair.public_key().as_ref());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock actor");
    let mock_addr = listener.local_addr().expect("local addr");
    let actor_url = format!("http://{}/actors/alice", mock_addr);
    let actor_url_for_handler = actor_url.clone();
    let public_pem_for_handler = public_pem.clone();

    let server = HttpServer::new(move || {
        let actor_url = actor_url_for_handler.clone();
        let public_pem = public_pem_for_handler.clone();
        App::new().route(
            "/actors/alice",
            web::get().to(move || {
                let actor_url = actor_url.clone();
                let public_pem = public_pem.clone();
                async move {
                    HttpResponse::Ok().json(json!({
                        "@context": ["https://www.w3.org/ns/activitystreams", "https://w3id.org/security/v1"],
                        "id": actor_url,
                        "type": "Person",
                        "preferredUsername": "alice",
                        "publicKey": {
                            "id": format!("{}#main-key", actor_url),
                            "owner": actor_url,
                            "publicKeyPem": public_pem,
                        }
                    }))
                }
            }),
        )
    })
    .listen(listener)
    .expect("listen mock actor")
    .run();

    let handle = server.handle();
    tokio::spawn(server);

    let activity = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/{}", actor_url, Uuid::new_v4()),
        "type": "Create",
        "actor": actor_url,
        "object": {
            "id": format!("{}/objects/{}", actor_url, Uuid::new_v4()),
            "type": "Note",
            "content": "hello from remote"
        }
    });
    let body = serde_json::to_vec(&activity).expect("activity json");

    let digest = format!(
        "SHA-256={}",
        base64::engine::general_purpose::STANDARD
            .encode(ring::digest::digest(&ring::digest::SHA256, &body).as_ref())
    );
    let date = chrono::Utc::now().to_rfc2822();

    let base_host = reqwest::Url::parse(&base)
        .expect("base url parse")
        .host_str()
        .expect("base host")
        .to_string();
    let host = if let Some(port) = reqwest::Url::parse(&base).expect("base parse").port() {
        format!("{base_host}:{port}")
    } else {
        base_host
    };

    let canonical = format!(
        "(request-target): post /users/{}/inbox\nhost: {}\ndate: {}\ndigest: {}",
        recipient_username, host, date, digest
    );
    let signature =
        base64::engine::general_purpose::STANDARD.encode(pair.sign(canonical.as_bytes()).as_ref());
    let signature_header = format!(
        "keyId=\"{}#main-key\",algorithm=\"ed25519\",headers=\"(request-target) host date digest\",signature=\"{}\"",
        activity["actor"].as_str().expect("actor str"),
        signature
    );

    let inbox_url = format!("{base}/users/{recipient_username}/inbox");

    let first: Value = client
        .post(&inbox_url)
        .header("content-type", "application/activity+json")
        .header("date", &date)
        .header("digest", &digest)
        .header("signature", &signature_header)
        .body(body.clone())
        .send()
        .await
        .expect("first inbox send failed")
        .json()
        .await
        .expect("first inbox json");
    assert_eq!(first["status"], "accepted");

    let second: Value = client
        .post(&inbox_url)
        .header("content-type", "application/activity+json")
        .header("date", &date)
        .header("digest", &digest)
        .header("signature", &signature_header)
        .body(body)
        .send()
        .await
        .expect("second inbox send failed")
        .json()
        .await
        .expect("second inbox json");
    assert_eq!(second["status"], "duplicate");

    handle.stop(true).await;
}

#[tokio::test]
async fn federation_send_records_delivery_failure_in_outbox() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (username, email, password) = register_user(&client, &base).await;
    let access_token = login(&client, &base, &email, &password).await;

    let res: Value = client
        .post(format!("{base}/federation/send"))
        .bearer_auth(&access_token)
        .json(&json!({
            "to_inboxes": ["http://127.0.0.1:9/inbox"],
            "content": "federation test"
        }))
        .send()
        .await
        .expect("federation send failed")
        .json()
        .await
        .expect("federation send json failed");

    let result_status = res["results"][0]["status"].as_str().unwrap_or_default();
    assert!(
        result_status == "failed" || result_status == "dead_letter",
        "unexpected delivery result status: {result_status}"
    );

    let outbox: Value = client
        .get(format!("{base}/users/{username}/outbox"))
        .send()
        .await
        .expect("outbox fetch failed")
        .json()
        .await
        .expect("outbox json failed");

    let empty = Vec::new();
    let statuses: Vec<&str> = outbox["orderedItems"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|v| v.get("status").and_then(|s| s.as_str()))
        .collect();
    assert!(
        statuses
            .iter()
            .any(|s| *s == "failed" || *s == "dead_letter"),
        "expected failed/dead_letter status in outbox"
    );
}

#[tokio::test]
async fn federation_send_delivers_to_mock_remote_inbox_and_records_outbox_success() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (username, email, password) = register_user(&client, &base).await;
    let access_token = login(&client, &base, &email, &password).await;

    let received_count = Arc::new(AtomicUsize::new(0));
    let received_count_for_handler = Arc::clone(&received_count);

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock inbox");
    let mock_addr = listener.local_addr().expect("mock inbox local addr");
    let inbox_url = format!("http://{mock_addr}/inbox");

    let server = HttpServer::new(move || {
        let received_count = Arc::clone(&received_count_for_handler);
        App::new().route(
            "/inbox",
            web::post().to(move |req: actix_web::HttpRequest, payload: web::Json<Value>| {
                let received_count = Arc::clone(&received_count);
                async move {
                    assert!(req.headers().contains_key("signature"));
                    assert!(req.headers().contains_key("digest"));
                    assert!(req.headers().contains_key("date"));
                    assert_eq!(payload["type"], "Create");
                    received_count.fetch_add(1, Ordering::Relaxed);
                    HttpResponse::Accepted().finish()
                }
            }),
        )
    })
    .listen(listener)
    .expect("listen mock inbox")
    .run();

    let handle = server.handle();
    tokio::spawn(server);

    let res: Value = client
        .post(format!("{base}/federation/send"))
        .bearer_auth(&access_token)
        .json(&json!({
            "to_inboxes": [inbox_url],
            "content": "interop success delivery"
        }))
        .send()
        .await
        .expect("federation send failed")
        .json()
        .await
        .expect("federation send json failed");

    assert_eq!(res["results"][0]["status"], "delivered");

    let outbox: Value = client
        .get(format!("{base}/users/{username}/outbox"))
        .send()
        .await
        .expect("outbox fetch failed")
        .json()
        .await
        .expect("outbox json failed");

    let empty = Vec::new();
    let statuses: Vec<&str> = outbox["orderedItems"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|v| v.get("status").and_then(|s| s.as_str()))
        .collect();
    assert!(
        statuses.contains(&"delivered"),
        "expected delivered status in outbox"
    );

    assert_eq!(received_count.load(Ordering::Relaxed), 1);

    handle.stop(true).await;
}

#[test]
fn federation_fixture_files_are_valid_json_and_signature_template() {
    let actor_fixture = include_str!("fixtures/federation/sample_actor.json");
    let create_note_fixture = include_str!("fixtures/federation/sample_create_note.json");
    let signature_fixture = include_str!("fixtures/federation/sample_signature_header.txt");

    let actor_json: Value =
        serde_json::from_str(actor_fixture).expect("actor fixture must be json");
    let note_json: Value =
        serde_json::from_str(create_note_fixture).expect("create note fixture must be json");

    assert_eq!(actor_json["type"], "Person");
    assert_eq!(note_json["type"], "Create");
    assert!(signature_fixture.contains("keyId="));
    assert!(signature_fixture.contains("signature="));
}
