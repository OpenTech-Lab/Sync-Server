use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn unique_username() -> String {
    format!("sticker_{}", Uuid::new_v4().simple())
}

fn unique_email() -> String {
    format!("{}@example.com", unique_username())
}

async fn register_and_login(client: &Client, base: &str) -> (String, String) {
    let email = unique_email();
    let username = unique_username();
    let password = "test_pass_secure!";

    let registered: Value = client
        .post(format!("{base}/auth/register"))
        .json(&json!({
            "username": username,
            "email": email,
            "password": password,
        }))
        .send()
        .await
        .expect("register failed")
        .json()
        .await
        .expect("register json invalid");

    let user_id = registered["id"].as_str().expect("missing id").to_string();

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login failed")
        .json()
        .await
        .expect("login json invalid");

    let access_token = login["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_string();

    (access_token, user_id)
}

#[tokio::test]
async fn upload_requires_authentication() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let res = client
        .post(format!("{base}/api/stickers/upload"))
        .json(&json!({
            "name": "smile",
            "mime_type": "image/png",
            "content_base64": "aGVsbG8=",
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn upload_validation_rejects_invalid_mime_type() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let (token, _user_id) = register_and_login(&client, &base).await;

    let res = client
        .post(format!("{base}/api/stickers/upload"))
        .bearer_auth(token)
        .json(&json!({
            "name": "bad",
            "mime_type": "text/plain",
            "content_base64": "aGVsbG8=",
        }))
        .send()
        .await
        .expect("upload failed");

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn sticker_sync_flow_list_then_get() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (token_b, _id_b) = register_and_login(&client, &base).await;

    let created: Value = client
        .post(format!("{base}/api/stickers/upload"))
        .bearer_auth(&token_a)
        .json(&json!({
            "name": "local-pending",
            "mime_type": "image/png",
            "content_base64": "aGVsbG8=",
        }))
        .send()
        .await
        .expect("upload failed")
        .json()
        .await
        .expect("upload json invalid");

    let sticker_id = created["id"].as_str().expect("missing sticker id");

    let list_a: Value = client
        .get(format!("{base}/api/stickers/list"))
        .bearer_auth(&token_a)
        .send()
        .await
        .expect("list failed")
        .json()
        .await
        .expect("list json invalid");

    let items_a = list_a.as_array().expect("list should be array");
    assert!(items_a.iter().any(|item| item["id"] == sticker_id));

    let get_a = client
        .get(format!("{base}/api/stickers/{sticker_id}"))
        .bearer_auth(&token_a)
        .send()
        .await
        .expect("get failed");
    assert_eq!(get_a.status(), 200);

    let get_b = client
        .get(format!("{base}/api/stickers/{sticker_id}"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("get other failed");
    assert_eq!(get_b.status(), 403);
}

#[tokio::test]
async fn non_admin_cannot_moderate_sticker() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;

    let created: Value = client
        .post(format!("{base}/api/stickers/upload"))
        .bearer_auth(&token_a)
        .json(&json!({
            "name": "needs-review",
            "mime_type": "image/png",
            "content_base64": "aGVsbG8=",
        }))
        .send()
        .await
        .expect("upload failed")
        .json()
        .await
        .expect("upload json invalid");

    let sticker_id = created["id"].as_str().expect("missing sticker id");

    let res = client
        .post(format!("{base}/api/stickers/{sticker_id}/moderate"))
        .bearer_auth(&token_a)
        .json(&json!({ "action": "approve" }))
        .send()
        .await
        .expect("moderate failed");

    assert_eq!(res.status(), 403);
}
