//! Каналы и категории сервера, доступ к каналам, вход в голос (фаза 9.1).

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use livekit_api::access_token::TokenVerifier;
use proto::{Channel, ChannelLayout, JoinResponse, Server};
use serde_json::{Value, json};

async fn layout(t: &T, tok: &str, server: i64) -> ChannelLayout {
    let (s, v) = t.call(Method::GET, &format!("/servers/{server}/channels"), Some(tok), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    serde_json::from_value(v).unwrap()
}

async fn new_channel(t: &T, tok: &str, server: i64, body: Value) -> (StatusCode, Value) {
    t.call(Method::POST, &format!("/servers/{server}/channels"), Some(tok), Some(body)).await
}

fn names(l: &ChannelLayout) -> Vec<(&str, Option<i64>)> {
    let mut c: Vec<&Channel> = l.channels.iter().collect();
    c.sort_by_key(|c| c.position);
    c.iter().map(|c| (c.name.as_str(), c.category_id)).collect()
}

/// Два сервера: у okabe «Лаб», у suzuha «Чужой».
async fn two(t: &T) -> ((String, i64), Server, (String, i64), Server) {
    let a = t.user("okabe").await;
    let b = t.user("suzuha").await;
    let sa = t.create_server(&a.0, "Лаб").await;
    let sb = t.create_server(&b.0, "Чужой").await;
    (a, sa, b, sb)
}

#[tokio::test]
async fn default_layout() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let l = layout(&t, &a.0, sa.id).await;
    assert_eq!(names(&l), vec![("general", None), ("General", None)]);
    assert!(l.channels.iter().all(|c| c.server_id == sa.id));
}

#[tokio::test]
async fn other_server_channel_is_404() {
    let t = setup().await;
    let (a, _, b, sb) = two(&t).await;
    let lb = layout(&t, &b.0, sb.id).await;
    let text = lb.channels.iter().find(|c| c.name == "general").unwrap().id;
    let voice = lb.channels.iter().find(|c| c.name == "General").unwrap().id;
    // картинка в чужом чате
    let (s, v, _, _) = t.call_raw(Method::POST, "/uploads", Some(&b.0), include_bytes!("../../client/assets/icon-32.png").to_vec()).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let att = v["id"].as_i64().unwrap();
    let (s, _) = t.call(Method::POST, &format!("/channels/{text}/messages"), Some(&b.0), Some(json!({"content": "x", "attachments": [att]}))).await;
    assert_eq!(s, StatusCode::CREATED);
    for (m, uri, body) in [
        (Method::GET, format!("/channels/{text}/messages"), None),
        (Method::POST, format!("/channels/{text}/messages"), Some(json!({"content": "взлом"}))),
        (Method::POST, format!("/channels/{voice}/join"), None),
        (Method::GET, format!("/attachments/{att}"), None),
        (Method::GET, format!("/servers/{}/channels", sb.id), None),
        (Method::PATCH, format!("/channels/{text}"), Some(json!({"name": "x"}))),
        (Method::DELETE, format!("/channels/{text}"), None),
    ] {
        let (s, _) = t.call(m, &uri, Some(&a.0), body).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn member_cannot_create_channel_admin_can() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let (member, mid) = t.user("daru").await;
    t.add_member(sa.id, mid, 0).await;
    let (s, _) = new_channel(&t, &member, sa.id, json!({"name": "x", "kind": "text"})).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = new_channel(&t, &a.0, sa.id, json!({"name": "Игры", "kind": "voice", "user_limit": 5})).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let ch: Channel = serde_json::from_value(v).unwrap();
    assert_eq!((ch.server_id, ch.user_limit, ch.position), (sa.id, Some(5), 2));
}

#[tokio::test]
async fn text_channel_limit_rejected() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let (s, _) = new_channel(&t, &a.0, sa.id, json!({"name": "x", "kind": "text", "user_limit": 3})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = new_channel(&t, &a.0, sa.id, json!({"name": "x", "kind": "voice", "user_limit": 100})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn rename_and_limit() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let voice = layout(&t, &a.0, sa.id).await.channels.into_iter().find(|c| c.kind == proto::ChannelKind::Voice).unwrap();
    let uri = format!("/channels/{}", voice.id);
    let (s, v) = t.call(Method::PATCH, &uri, Some(&a.0), Some(json!({"name": "Болталка", "user_limit": 4}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, v) = t.call(Method::PATCH, &uri, Some(&a.0), Some(json!({"user_limit": null}))).await;
    assert_eq!(s, StatusCode::OK);
    let ch: Channel = serde_json::from_value(v).unwrap();
    assert_eq!((ch.name.as_str(), ch.user_limit), ("Болталка", None));
}

#[tokio::test]
async fn layout_reorders_and_moves_between_categories() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let (s, v) = t.call(Method::POST, &format!("/servers/{}/categories", sa.id), Some(&a.0), Some(json!({"name": "Голос"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let cat = v["id"].as_i64().unwrap();
    let l = layout(&t, &a.0, sa.id).await;
    let text = l.channels.iter().find(|c| c.name == "general").unwrap().id;
    let voice = l.channels.iter().find(|c| c.name == "General").unwrap().id;
    let (s, v) = t
        .call(Method::PUT, &format!("/servers/{}/layout", sa.id), Some(&a.0), Some(json!({
            "categories": [cat],
            "channels": [{"id": voice, "category_id": cat}, {"id": text}]
        })))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let l = layout(&t, &a.0, sa.id).await;
    assert_eq!(names(&l), vec![("General", Some(cat)), ("general", None)]);
    assert_eq!(l.categories.len(), 1);
}

#[tokio::test]
async fn layout_rejects_foreign_ids() {
    let t = setup().await;
    let (a, sa, b, sb) = two(&t).await;
    let foreign = layout(&t, &b.0, sb.id).await.channels[0].id;
    let mine = layout(&t, &a.0, sa.id).await.channels;
    let (s, _) = t
        .call(Method::PUT, &format!("/servers/{}/layout", sa.id), Some(&a.0), Some(json!({
            "categories": [],
            "channels": [{"id": foreign}, {"id": mine[0].id}, {"id": mine[1].id}]
        })))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = t
        .call(Method::PUT, &format!("/servers/{}/layout", sa.id), Some(&a.0), Some(json!({"categories": [], "channels": [{"id": mine[0].id}]})))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "нужны все каналы сервера");
}

#[tokio::test]
async fn delete_category_keeps_channels() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let (_, v) = t.call(Method::POST, &format!("/servers/{}/categories", sa.id), Some(&a.0), Some(json!({"name": "Текст"}))).await;
    let cat = v["id"].as_i64().unwrap();
    let (s, v) = new_channel(&t, &a.0, sa.id, json!({"name": "мемы", "kind": "text", "category_id": cat})).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (s, _) = t.call(Method::PATCH, &format!("/categories/{cat}"), Some(&a.0), Some(json!({"name": "Болтовня"}))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = t.call(Method::DELETE, &format!("/categories/{cat}"), Some(&a.0), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let l = layout(&t, &a.0, sa.id).await;
    assert!(l.categories.is_empty());
    assert_eq!(l.channels.len(), 3);
    assert!(l.channels.iter().all(|c| c.category_id.is_none()));
}

#[tokio::test]
async fn join_room_name_and_limit() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let (b, bid) = t.user("daru").await;
    t.add_member(sa.id, bid, 0).await;
    let voice = layout(&t, &a.0, sa.id).await.channels.into_iter().find(|c| c.kind == proto::ChannelKind::Voice).unwrap();
    let room = format!("srv-{}-ch-{}", sa.id, voice.id);
    let (s, v) = t.call(Method::POST, &format!("/channels/{}/join", voice.id), Some(&b), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let j: JoinResponse = serde_json::from_value(v).unwrap();
    assert_eq!(j.room, room);
    let claims = TokenVerifier::with_api_key(LK_KEY, LK_SECRET).verify(&j.livekit_token).unwrap();
    assert_eq!(claims.video.room, room);
    assert_eq!(claims.name, "daru");
    // лимит 1, внутри уже okabe → daru не пускаем
    t.call(Method::PATCH, &format!("/channels/{}", voice.id), Some(&a.0), Some(json!({"user_limit": 1}))).await;
    t.set_voice(&room, &[a.1]).await;
    let (s, v) = t.call(Method::POST, &format!("/channels/{}/join", voice.id), Some(&b), None).await;
    assert_eq!((s, err(&v)), (StatusCode::CONFLICT, "channel_full".to_string()));
    // тот, кто уже внутри (переподключение), проходит
    let (s, _) = t.call(Method::POST, &format!("/channels/{}/join", voice.id), Some(&a.0), None).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn delete_voice_channel_clears_room() {
    let t = setup().await;
    let (a, sa, _, _) = two(&t).await;
    let voice = layout(&t, &a.0, sa.id).await.channels.into_iter().find(|c| c.kind == proto::ChannelKind::Voice).unwrap();
    let room = format!("srv-{}-ch-{}", sa.id, voice.id);
    t.set_voice(&room, &[a.1]).await;
    let (s, _) = t.call(Method::DELETE, &format!("/channels/{}", voice.id), Some(&a.0), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(t.removed(), vec![(room, a.1.to_string())]);
}

#[tokio::test]
async fn legacy_get_channels_returns_server_1() {
    let t = setup().await;
    let admin = t.admin().await;
    let (s, v) = t.call(Method::POST, "/channels", Some(&admin), Some(json!({"name": "Общий", "kind": "voice"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (other, _) = t.user("suzuha").await;
    let srv = t.create_server(&other, "Чужой").await;
    let _ = srv;
    let (s, v) = t.call(Method::GET, "/channels", Some(&admin), None).await;
    assert_eq!(s, StatusCode::OK);
    let list: Vec<Channel> = serde_json::from_value(v).unwrap();
    assert_eq!(list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Общий"]);
    let (s, v) = t.call(Method::GET, "/channels", Some(&other), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v, json!([]), "не участник сервера №1 — пусто");
}
