//! Серверы, участники, роли, баны, приглашения (фаза 9.1).

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use proto::{Member, Server};
use serde_json::json;

/// Заголовок PNG нужного размера (сервер смотрит только на заголовок).
pub fn fake_png(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    v.extend_from_slice(&[0u8; 64]);
    v
}

async fn servers_of(t: &T, tok: &str) -> Vec<Server> {
    let (s, v) = t.call(Method::GET, "/servers", Some(tok), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    serde_json::from_value(v).unwrap()
}

#[tokio::test]
async fn create_server_makes_owner_and_default_channels() {
    let t = setup().await;
    let (tok, me) = t.user("okabe").await;
    let srv = t.create_server(&tok, "Лаборатория").await;
    assert_eq!(srv.owner_id, me);
    assert_eq!(srv.name, "Лаборатория");
    let mut pos: Vec<i32> = srv.roles.iter().map(|r| r.position).collect();
    pos.sort();
    assert_eq!(pos, vec![0, 1, 2, 3]);
    let mine = srv.roles.iter().find(|r| r.id == srv.my_role_id).unwrap();
    assert_eq!(mine.position, 3);
    let chans: Vec<(String, String)> = sqlx::query_as("SELECT name, kind FROM channels WHERE server_id = ? ORDER BY position")
        .bind(srv.id).fetch_all(&t.pool).await.unwrap();
    assert_eq!(chans, vec![("general".into(), "text".into()), ("General".into(), "voice".into())]);
    assert_eq!(servers_of(&t, &tok).await, vec![srv]);
}

#[tokio::test]
async fn name_validated() {
    let t = setup().await;
    let (tok, _) = t.user("okabe").await;
    let (s, v) = t.call(Method::POST, "/servers", Some(&tok), Some(json!({"name": "   "}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let long = "я".repeat(51);
    let (s, _) = t.call(Method::POST, "/servers", Some(&tok), Some(json!({"name": long}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn member_cannot_rename_admin_can() {
    let t = setup().await;
    let (owner, _) = t.user("okabe").await;
    let (member, mid) = t.user("daru").await;
    let (admin, aid) = t.user("kurisu").await;
    let srv = t.create_server(&owner, "Лаб").await;
    t.add_member(srv.id, mid, 0).await;
    t.add_member(srv.id, aid, 2).await;
    let uri = format!("/servers/{}", srv.id);
    let (s, _) = t.call(Method::PATCH, &uri, Some(&member), Some(json!({"name": "Взлом"}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = t.call(Method::PATCH, &uri, Some(&admin), Some(json!({"name": "Лаборатория"}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(serde_json::from_value::<Server>(v).unwrap().name, "Лаборатория");
}

#[tokio::test]
async fn non_member_gets_404() {
    let t = setup().await;
    let (owner, _) = t.user("okabe").await;
    let (stranger, _) = t.user("suzuha").await;
    let srv = t.create_server(&owner, "Лаб").await;
    for (m, uri) in [(Method::GET, format!("/servers/{}/members", srv.id)), (Method::PATCH, format!("/servers/{}", srv.id)), (Method::DELETE, format!("/servers/{}", srv.id))] {
        let (s, _) = t.call(m, &uri, Some(&stranger), Some(json!({"name": "x"}))).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
    }
    assert!(servers_of(&t, &stranger).await.is_empty());
}

#[tokio::test]
async fn members_list() {
    let t = setup().await;
    let (owner, oid) = t.user("okabe").await;
    let (_, mid) = t.user("daru").await;
    let srv = t.create_server(&owner, "Лаб").await;
    t.add_member(srv.id, mid, 0).await;
    let (s, v) = t.call(Method::GET, &format!("/servers/{}/members", srv.id), Some(&owner), None).await;
    assert_eq!(s, StatusCode::OK);
    let mut ms: Vec<Member> = serde_json::from_value(v).unwrap();
    ms.sort_by_key(|m| m.user.id);
    assert_eq!(ms.iter().map(|m| (m.user.id, m.user.name.as_str())).collect::<Vec<_>>(), vec![(oid, "okabe"), (mid, "daru")]);
}

#[tokio::test]
async fn owner_cannot_leave_member_can() {
    let t = setup().await;
    let (owner, _) = t.user("okabe").await;
    let (member, mid) = t.user("daru").await;
    let srv = t.create_server(&owner, "Лаб").await;
    t.add_member(srv.id, mid, 0).await;
    let uri = format!("/servers/{}/members/@me", srv.id);
    let (s, _) = t.call(Method::DELETE, &uri, Some(&owner), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = t.call(Method::DELETE, &uri, Some(&member), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(servers_of(&t, &member).await.is_empty());
}

#[tokio::test]
async fn transfer_ownership_swaps_roles() {
    let t = setup().await;
    let (owner, oid) = t.user("okabe").await;
    let (heir, hid) = t.user("kurisu").await;
    let srv = t.create_server(&owner, "Лаб").await;
    t.add_member(srv.id, hid, 0).await;
    let (s, _) = t.call(Method::POST, &format!("/servers/{}/transfer", srv.id), Some(&heir), Some(json!({"user_id": hid}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "передать может только владелец");
    let (s, v) = t.call(Method::POST, &format!("/servers/{}/transfer", srv.id), Some(&owner), Some(json!({"user_id": hid}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let now = &servers_of(&t, &heir).await[0];
    assert_eq!(now.owner_id, hid);
    assert_eq!(now.roles.iter().find(|r| r.id == now.my_role_id).unwrap().position, 3);
    let old = &servers_of(&t, &owner).await[0];
    assert_eq!(old.roles.iter().find(|r| r.id == old.my_role_id).unwrap().position, 2);
    let _ = oid;
}

#[tokio::test]
async fn icon_upload_validates_and_replaces() {
    let t = setup().await;
    let (owner, _) = t.user("okabe").await;
    let srv = t.create_server(&owner, "Лаб").await;
    let uri = format!("/servers/{}/icon", srv.id);
    let (s, _, _, _) = t.call_raw(Method::PUT, &uri, Some(&owner), fake_png(256, 200)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "не квадрат");
    let (s, _, _, _) = t.call_raw(Method::PUT, &uri, Some(&owner), fake_png(1024, 1024)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "больше 512");
    let (s, v, _, _) = t.call_raw(Method::PUT, &uri, Some(&owner), fake_png(256, 256)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let k1 = serde_json::from_value::<Server>(v).unwrap().icon.unwrap();
    assert!(t.uploads.join(&k1).exists());
    let (s, v, _, _) = t.call_raw(Method::PUT, &uri, Some(&owner), fake_png(128, 128)).await;
    assert_eq!(s, StatusCode::OK);
    let k2 = serde_json::from_value::<Server>(v).unwrap().icon.unwrap();
    assert!(!t.uploads.join(&k1).exists(), "старая иконка удалена");
    let (s, _, _, body) = t.call_raw(Method::GET, &format!("/images/{k2}"), Some(&owner), vec![]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(&body[..4], b"\x89PNG");
}

#[tokio::test]
async fn delete_server_removes_files_and_voice() {
    let t = setup().await;
    let (owner, _) = t.user("okabe").await;
    let (admin, aid) = t.user("kurisu").await;
    let srv = t.create_server(&owner, "Лаб").await;
    t.add_member(srv.id, aid, 2).await;
    let (_, v, _, _) = t.call_raw(Method::PUT, &format!("/servers/{}/icon", srv.id), Some(&owner), fake_png(64, 64)).await;
    let key = serde_json::from_value::<Server>(v).unwrap().icon.unwrap();
    let voice: i64 = sqlx::query_scalar("SELECT id FROM channels WHERE server_id = ? AND kind = 'voice'")
        .bind(srv.id).fetch_one(&t.pool).await.unwrap();
    let room = format!("srv-{}-ch-{voice}", srv.id);
    t.set_voice(&room, &[aid]).await;
    let (s, _) = t.call(Method::DELETE, &format!("/servers/{}", srv.id), Some(&admin), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "удалить может только владелец");
    let (s, _) = t.call(Method::DELETE, &format!("/servers/{}", srv.id), Some(&owner), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(!t.uploads.join(&key).exists());
    assert_eq!(t.removed(), vec![(room, aid.to_string())]);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM channels WHERE server_id = ?").bind(srv.id).fetch_one(&t.pool).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn server_limit_per_owner() {
    let t = setup().await;
    let (tok, _) = t.user("okabe").await;
    for i in 0..proto::limits::SERVERS_OWNED_MAX {
        t.create_server(&tok, &format!("s{i}")).await;
    }
    let (s, _) = t.call(Method::POST, "/servers", Some(&tok), Some(json!({"name": "лишний"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}
