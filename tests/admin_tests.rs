use reqwest::Client;
use serde_json::json;
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn unique_username() -> String {
    format!("admin_test_{}", Uuid::new_v4().simple())
}

fn unique_email() -> String {
    format!("{}@example.com", unique_username())
}

async fn register_user(client: &Client, base: &str) -> (String, String) {
    let email = unique_email();
    let password = "test_pass_secure!".to_string();
    let username = unique_username();

    let res = client
        .post(format!("{base}/auth/register"))
        .json(&json!({
            "username": username,
            "email": email,
            "password": password,
        }))
        .send()
        .await
        .expect("register failed");

    assert!(
        res.status().is_success(),
        "register failed: {}",
        res.status()
    );
    (email, password)
}

async fn login(client: &Client, base: &str, email: &str, password: &str) -> String {
    let response = client
        .post(format!("{base}/auth/login"))
        .json(&json!({
            "email": email,
            "password": password,
        }))
        .send()
        .await
        .expect("login failed");

    let body: serde_json::Value = response.json().await.expect("login json");
    body["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_string()
}

#[tokio::test]
async fn admin_overview_requires_authentication() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let res = client
        .get(format!("{base}/api/admin/overview"))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn admin_config_forbids_non_admin_users() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (email, password) = register_user(&client, &base).await;
    let access_token = login(&client, &base, &email, &password).await;

    let res = client
        .get(format!("{base}/api/admin/config"))
        .bearer_auth(access_token)
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 403);
}
