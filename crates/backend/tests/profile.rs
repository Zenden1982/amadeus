//! Профиль: отображаемое имя и аватар (фаза 9.1).

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::*;
use livekit_api::access_token::TokenVerifier;
use proto::{ChannelLayout, JoinResponse, Message, ServerEvent, User};
use serde_json::json;
use tower::ServiceExt;

fn fake_png(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    v
}

async fn me(t: &T, tok: &str) -> User {
    let (s, v) = t.call(Method::GET, "/me", Some(tok), None).await;
    assert_eq!(s, StatusCode::OK);
    serde_json::from_value(v).unwrap()
}

#[tokio::test]
async fn display_name_set_and_reset() {
    let t = setup().await;
    let (tok, _) = t.user("okabe").await;
    let (s, v) = t.call(Method::PATCH, "/me", Some(&tok), Some(json!({"display_name": "  Окабэ Ринтаро "}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(serde_json::from_value::<User>(v).unwrap().display_name.as_deref(), Some("Окабэ Ринтаро"));
    assert_eq!(me(&t, &tok).await.display_name.as_deref(), Some("Окабэ Ринтаро"));
    let login = t.login("okabe", "password123").await;
    assert_eq!(me(&t, &login).await.display_name.as_deref(), Some("Окабэ Ринтаро"));
    let (s, _) = t.call(Method::PATCH, "/me", Some(&tok), Some(json!({"display_name": ""}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me(&t, &tok).await.display_name, None);
}

#[tokio::test]
async fn display_name_too_long() {
    let t = setup().await;
    let (tok, _) = t.user("okabe").await;
    let (s, _) = t.call(Method::PATCH, "/me", Some(&tok), Some(json!({"display_name": "я".repeat(33)}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn avatar_upload_replace_delete() {
    let t = setup().await;
    let (tok, _) = t.user("okabe").await;
    let (s, v, _, _) = t.call_raw(Method::PUT, "/me/avatar", Some(&tok), fake_png(200, 100)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let (s, v, _, _) = t.call_raw(Method::PUT, "/me/avatar", Some(&tok), fake_png(256, 256)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let k1 = serde_json::from_value::<User>(v).unwrap().avatar.unwrap();
    assert!(t.uploads.join(&k1).exists());
    let (_, v, _, _) = t.call_raw(Method::PUT, "/me/avatar", Some(&tok), fake_png(128, 128)).await;
    let k2 = serde_json::from_value::<User>(v).unwrap().avatar.unwrap();
    assert!(!t.uploads.join(&k1).exists());
    assert_eq!(me(&t, &tok).await.avatar.as_deref(), Some(k2.as_str()));
    let (s, _) = t.call(Method::DELETE, "/me/avatar", Some(&tok), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!t.uploads.join(&k2).exists());
    assert_eq!(me(&t, &tok).await.avatar, None);
}

#[tokio::test]
async fn author_and_token_use_display_name() {
    let t = setup().await;
    let (tok, _) = t.user("okabe").await;
    let srv = t.create_server(&tok, "Лаб").await;
    t.call(Method::PATCH, "/me", Some(&tok), Some(json!({"display_name": "Окабэ"}))).await;
    t.call_raw(Method::PUT, "/me/avatar", Some(&tok), fake_png(64, 64)).await;
    let (_, v) = t.call(Method::GET, &format!("/servers/{}/channels", srv.id), Some(&tok), None).await;
    let l: ChannelLayout = serde_json::from_value(v).unwrap();
    let text = l.channels.iter().find(|c| c.kind == proto::ChannelKind::Text).unwrap().id;
    let voice = l.channels.iter().find(|c| c.kind == proto::ChannelKind::Voice).unwrap().id;
    let (_, v) = t.call(Method::POST, &format!("/channels/{text}/messages"), Some(&tok), Some(json!({"content": "эль псай конгру"}))).await;
    let m: Message = serde_json::from_value(v).unwrap();
    assert_eq!(m.author.display_name.as_deref(), Some("Окабэ"));
    assert!(m.author.avatar.is_some());
    let (_, v) = t.call(Method::POST, &format!("/channels/{voice}/join"), Some(&tok), None).await;
    let j: JoinResponse = serde_json::from_value(v).unwrap();
    assert_eq!(TokenVerifier::with_api_key(LK_KEY, LK_SECRET).verify(&j.livekit_token).unwrap().name, "Окабэ");
    // участники сервера видят новое имя
    let (_, v) = t.call(Method::GET, &format!("/servers/{}/members", srv.id), Some(&tok), None).await;
    assert_eq!(v[0]["user"]["display_name"], "Окабэ");
}

#[tokio::test]
async fn user_updated_goes_to_shared_servers_only() {
    let t = setup().await;
    let a = t.user("okabe").await;
    let b = t.user("kurisu").await;
    let c = t.user("suzuha").await;
    let srv = t.create_server(&a.0, "Лаб").await;
    t.add_member(srv.id, b.1, 0).await;
    let open = |tok: String| {
        let app = t.app.clone();
        async move {
            let req = Request::builder().uri("/events").header(header::AUTHORIZATION, format!("Bearer {tok}")).body(Body::empty()).unwrap();
            let mut body = app.oneshot(req).await.unwrap().into_body();
            let mut buf = String::new();
            assert_eq!(next_event(&mut body, &mut buf).await, ServerEvent::Hello);
            (body, buf)
        }
    };
    let (mut bb, mut bbuf) = open(b.0.clone()).await;
    let (mut cb, mut cbuf) = open(c.0.clone()).await;
    t.call(Method::PATCH, "/me", Some(&a.0), Some(json!({"display_name": "Хооин Кёма"}))).await;
    match next_event(&mut bb, &mut bbuf).await {
        ServerEvent::UserUpdated { user } => assert_eq!(user.display_name.as_deref(), Some("Хооин Кёма")),
        e => panic!("{e:?}"),
    }
    let own = t.create_server(&c.0, "Своё").await;
    match next_event(&mut cb, &mut cbuf).await {
        ServerEvent::ServerUpserted { server } => assert_eq!(server.id, own.id),
        e => panic!("чужому пришло {e:?}"),
    }
}
