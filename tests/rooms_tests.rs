/// Integration tests for /api/rooms endpoints.
///
/// These tests require a running server and real database.
/// Set `TEST_BASE_URL` (e.g. `http://localhost:8080`) to run them.
/// If the env var is absent the tests are skipped.
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn unique_email() -> String {
    format!("room-test-{}@example.com", Uuid::new_v4())
}

/// Helper: register a user and return their access token + user_id.
async fn register_and_login(client: &Client, base: &str) -> (String, String) {
    let email = unique_email();
    let password = "test_pass_secure!";

    let reg: Value = client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("register failed")
        .json()
        .await
        .expect("register invalid JSON");

    let user_id = reg["id"].as_str().expect("missing id").to_owned();

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login failed")
        .json()
        .await
        .expect("login invalid JSON");

    let access_token = login["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_owned();

    (access_token, user_id)
}

#[tokio::test]
async fn create_room_returns_201_with_members() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;
    let (_token_c, id_c) = register_and_login(&client, &base).await;

    let res = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_a)
        .json(&json!({
            "name": "Test Room",
            "member_ids": [id_b, id_c]
        }))
        .send()
        .await
        .expect("create_room failed");

    assert_eq!(res.status(), 201, "expected 201 Created");
    let body: Value = res.json().await.expect("invalid JSON");

    assert!(body["id"].as_str().is_some(), "missing room id");
    assert_eq!(body["name"], "Test Room");
    assert_eq!(body["created_by"], id_a);
    assert_eq!(body["member_count"], 3);
    assert_eq!(body["unread_count"], 0);
    assert!(body["last_message_preview"].is_null());

    let members = body["members"].as_array().expect("members not array");
    assert_eq!(members.len(), 3);

    let creator_member = members
        .iter()
        .find(|m| m["user_id"] == id_a)
        .expect("creator not in members");
    assert_eq!(creator_member["role"], "owner");
}

#[tokio::test]
async fn create_room_empty_name_returns_400() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token, _id) = register_and_login(&client, &base).await;

    let res = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token)
        .json(&json!({
            "name": "   ",
            "member_ids": []
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 400, "empty room name should fail");
}

#[tokio::test]
async fn list_rooms_returns_rooms_for_member() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;

    client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_a)
        .json(&json!({ "name": "Room 1", "member_ids": [id_b] }))
        .send()
        .await
        .expect("create room 1 failed");

    client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_a)
        .json(&json!({ "name": "Room 2", "member_ids": [id_b] }))
        .send()
        .await
        .expect("create room 2 failed");

    let res = client
        .get(format!("{base}/api/rooms"))
        .bearer_auth(&token_a)
        .send()
        .await
        .expect("list_rooms failed");

    assert_eq!(res.status(), 200);
    let rooms: Value = res.json().await.expect("invalid JSON");
    let arr = rooms.as_array().expect("expected array");

    assert_eq!(arr.len(), 2, "should have 2 rooms");
}

#[tokio::test]
async fn send_room_message_updates_unread_counts() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, id_a) = register_and_login(&client, &base).await;
    let (token_b, id_b) = register_and_login(&client, &base).await;
    let (token_c, id_c) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_a)
        .json(&json!({"name": "Chat Room", "member_ids": [id_b, id_c]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    let res = client
        .post(format!("{base}/api/rooms/{room_id}/messages"))
        .bearer_auth(&token_a)
        .json(&json!({"content": "Hello everyone!"}))
        .send()
        .await
        .expect("send_message failed");

    assert_eq!(res.status(), 201);
    let msg: Value = res.json().await.expect("invalid JSON");
    assert_eq!(msg["content"], "Hello everyone!");
    assert_eq!(msg["sender_id"], id_a);

    let room_a: Value = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_a)
        .send()
        .await
        .expect("get room A failed")
        .json()
        .await
        .expect("invalid JSON");
    assert_eq!(room_a["unread_count"], 0);
    assert_eq!(room_a["last_message_preview"], "Hello everyone!");

    let room_b: Value = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("get room B failed")
        .json()
        .await
        .expect("invalid JSON");
    assert_eq!(room_b["unread_count"], 1);

    let room_c: Value = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_c)
        .send()
        .await
        .expect("get room C failed")
        .json()
        .await
        .expect("invalid JSON");
    assert_eq!(room_c["unread_count"], 1);
}

#[tokio::test]
async fn room_owner_can_add_members_after_create() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_owner, _owner_id) = register_and_login(&client, &base).await;
    let (_token_member, member_id) = register_and_login(&client, &base).await;
    let (_token_new_member, new_member_id) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_owner)
        .json(&json!({"name": "Invite Room", "member_ids": [member_id]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    let res = client
        .post(format!("{base}/api/rooms/{room_id}/members"))
        .bearer_auth(&token_owner)
        .json(&json!({"member_ids": [new_member_id]}))
        .send()
        .await
        .expect("invite failed");

    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.expect("invalid JSON");
    assert_eq!(updated["member_count"], 3);
    let members = updated["members"].as_array().expect("members not array");
    assert!(
        members
            .iter()
            .any(|member| member["user_id"] == new_member_id),
        "new member not found in room members"
    );
}

#[tokio::test]
async fn non_owner_cannot_add_members_after_create() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_owner, _owner_id) = register_and_login(&client, &base).await;
    let (token_member, member_id) = register_and_login(&client, &base).await;
    let (_token_new_member, new_member_id) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_owner)
        .json(&json!({"name": "Invite Room", "member_ids": [member_id]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    let res = client
        .post(format!("{base}/api/rooms/{room_id}/members"))
        .bearer_auth(&token_member)
        .json(&json!({"member_ids": [new_member_id]}))
        .send()
        .await
        .expect("invite failed");

    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn room_owner_can_remove_member_after_create() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_owner, owner_id) = register_and_login(&client, &base).await;
    let (token_member, member_id) = register_and_login(&client, &base).await;
    let (token_other, other_id) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_owner)
        .json(&json!({"name": "Remove Room", "member_ids": [member_id, other_id]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    let res = client
        .delete(format!("{base}/api/rooms/{room_id}/members/{member_id}"))
        .bearer_auth(&token_owner)
        .send()
        .await
        .expect("remove member failed");

    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.expect("invalid JSON");
    assert_eq!(updated["member_count"], 2);
    let members = updated["members"].as_array().expect("members not array");
    assert!(
        members.iter().all(|member| member["user_id"] != member_id),
        "removed member still appears in room detail"
    );
    assert!(
        members.iter().any(|member| member["user_id"] == owner_id),
        "owner should remain in room detail"
    );

    let removed_room_res = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_member)
        .send()
        .await
        .expect("removed member get room failed");
    assert_eq!(removed_room_res.status(), 404);

    let removed_rooms: Value = client
        .get(format!("{base}/api/rooms"))
        .bearer_auth(&token_member)
        .send()
        .await
        .expect("removed member list rooms failed")
        .json()
        .await
        .expect("invalid JSON");
    assert!(
        removed_rooms
            .as_array()
            .map(|rooms| rooms.is_empty())
            .unwrap_or(false),
        "removed member should no longer list the room"
    );

    let other_room_res = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_other)
        .send()
        .await
        .expect("other member get room failed");
    assert_eq!(other_room_res.status(), 200);
}

#[tokio::test]
async fn non_owner_cannot_remove_room_member() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_owner, _owner_id) = register_and_login(&client, &base).await;
    let (token_member, member_id) = register_and_login(&client, &base).await;
    let (_token_other, other_id) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_owner)
        .json(&json!({"name": "Remove Room", "member_ids": [member_id, other_id]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    let res = client
        .delete(format!("{base}/api/rooms/{room_id}/members/{other_id}"))
        .bearer_auth(&token_member)
        .send()
        .await
        .expect("remove member failed");

    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn room_owner_cannot_remove_self() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_owner, owner_id) = register_and_login(&client, &base).await;
    let (_token_member, member_id) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_owner)
        .json(&json!({"name": "Remove Room", "member_ids": [member_id]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    let res = client
        .delete(format!("{base}/api/rooms/{room_id}/members/{owner_id}"))
        .bearer_auth(&token_owner)
        .send()
        .await
        .expect("remove owner failed");

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn pagination_and_mark_read_work_correctly() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (token_b, id_b) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_a)
        .json(&json!({"name": "Test", "member_ids": [id_b]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    for i in 1..=5 {
        client
            .post(format!("{base}/api/rooms/{room_id}/messages"))
            .bearer_auth(&token_a)
            .json(&json!({"content": format!("msg {i}")}))
            .send()
            .await
            .expect("send room message failed");
    }

    let page1: Value = client
        .get(format!("{base}/api/rooms/{room_id}/messages?limit=2"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("page1 failed")
        .json()
        .await
        .expect("invalid JSON");

    let page1_arr = page1.as_array().expect("expected array");
    assert_eq!(page1_arr.len(), 2);
    assert_eq!(page1_arr[0]["content"], "msg 5");
    assert_eq!(page1_arr[1]["content"], "msg 4");

    let cursor = page1_arr[1]["id"].as_str().expect("cursor id").to_string();

    let page2: Value = client
        .get(format!(
            "{base}/api/rooms/{room_id}/messages?before={cursor}&limit=10"
        ))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("page2 failed")
        .json()
        .await
        .expect("invalid JSON");

    let page2_arr = page2.as_array().expect("expected array");
    assert_eq!(page2_arr.len(), 3);
    assert_eq!(page2_arr[0]["content"], "msg 3");
    assert_eq!(page2_arr[2]["content"], "msg 1");

    let room_before_mark_read: Value = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("room before mark read failed")
        .json()
        .await
        .expect("invalid JSON");
    assert_eq!(room_before_mark_read["unread_count"], 5);

    let mark_read: Value = client
        .post(format!("{base}/api/rooms/{room_id}/read"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("mark_read failed")
        .json()
        .await
        .expect("invalid JSON");
    assert_eq!(mark_read["count"], 5);

    let room_after_mark_read: Value = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("room after mark read failed")
        .json()
        .await
        .expect("invalid JSON");
    assert_eq!(room_after_mark_read["unread_count"], 0);
}

#[tokio::test]
async fn non_member_cannot_access_room() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;
    let (token_c, _id_c) = register_and_login(&client, &base).await;

    let room: Value = client
        .post(format!("{base}/api/rooms"))
        .bearer_auth(&token_a)
        .json(&json!({"name": "Private", "member_ids": [id_b]}))
        .send()
        .await
        .expect("create room failed")
        .json()
        .await
        .expect("invalid JSON");

    let room_id = room["id"].as_str().expect("missing room id");

    let res = client
        .get(format!("{base}/api/rooms/{room_id}"))
        .bearer_auth(&token_c)
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 403, "non-member should get 403 Forbidden");
}
