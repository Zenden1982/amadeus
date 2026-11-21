//! ЛС, непрочитанные и звонки в ЛС.

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use common::*;
use proto::{CallInfo, DmChannel, Message, ServerEvent, UnreadChannel};
use serde_json::json;
use tower::ServiceExt;

struct Stream {
    body: Body,
    buf: String,
}

impl Stream {
    async fn open(t: &T, tok: &str) -> Stream {
        let req = Request::builder().uri("/events").header(header::AUTHORIZATION, format!("Bearer {tok}")).body(Body::empty()).unwrap();
        let resp = t.app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let mut s = Stream { body: resp.into_body(), buf: String::new() };
        assert_eq!(s.next().await, ServerEvent::Hello);
        s
    }
    async fn next(&mut self) -> ServerEvent {
        next_event(&mut self.body, &mut self.buf).await
    }
    /// Следующее событие нужного вида (presence и прочие пропускаем).
    async fn next_where(&mut self, f: impl Fn(&ServerEvent) -> bool) -> ServerEvent {
        loop {
            let e = self.next().await;
            if f(&e) {
                return e;
            }
        }
    }
    async fn next_call(&mut self) -> Option<CallInfo> {
        match self.next_where(|e| matches!(e, ServerEvent::Call { .. })).await {
            ServerEvent::Call { call, .. } => call,
            _ => unreachable!(),
        }
    }
}

async fn befriend(t: &T, a: &str, b_name: &str, a_id: i64, b: &str) {
    let (s, v) = t.call(Method::POST, "/friends/requests", Some(a), Some(json!({"name": b_name}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (s, v) = t.call(Method::POST, &format!("/friends/{a_id}/accept"), Some(b), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
}

async fn open_dm(t: &T, tok: &str, other: i64) -> (StatusCode, serde_json::Value) {
    t.call(Method::POST, "/dms", Some(tok), Some(json!({"user_id": other}))).await
}

async fn send(t: &T, tok: &str, ch: i64, text: &str) -> (StatusCode, serde_json::Value) {
    t.call(Method::POST, &format!("/channels/{ch}/messages"), Some(tok), Some(json!({"content": text}))).await
}

async fn dms(t: &T, tok: &str) -> Vec<DmChannel> {
    let (s, v) = t.call(Method::GET, "/dms", Some(tok), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    serde_json::from_value(v).unwrap()
}

async fn unread(t: &T, tok: &str) -> Vec<UnreadChannel> {
    let (s, v) = t.call(Method::GET, "/unread", Some(tok), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    serde_json::from_value(v).unwrap()
}

#[tokio::test]
async fn dm_between_friends() {
    let t = setup().await;
    let (a, a_id) = t.user("okabe").await;
    let (b, b_id) = t.user("kurisu").await;
    let (c, _) = t.user("daru").await;
    befriend(&t, &a, "kurisu", a_id, &b).await;

    let (s, v) = open_dm(&t, &a, b_id).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let dm: DmChannel = serde_json::from_value(v).unwrap();
    assert_eq!(dm.user.id, b_id);
    assert_eq!(dm.last_message_id, None);
    // повторно — тот же диалог, с другой стороны — тоже
    let (s, v) = open_dm(&t, &a, b_id).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["id"], dm.id);
    // пустой диалог видит только открывший
    assert_eq!(dms(&t, &a).await.len(), 1);
    assert!(dms(&t, &b).await.is_empty());

    let mut sb = Stream::open(&t, &b).await;
    let (s, v) = send(&t, &a, dm.id, "Эль Псай Конгру").await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let msg: Message = serde_json::from_value(v).unwrap();
    let e = sb.next_where(|e| matches!(e, ServerEvent::MessageCreated { .. })).await;
    assert!(matches!(e, ServerEvent::MessageCreated { message } if message.id == msg.id && message.channel_id == dm.id));

    let lb = dms(&t, &b).await;
    assert_eq!(lb.len(), 1);
    assert_eq!((lb[0].user.id, lb[0].last_message_id), (a_id, Some(msg.id)));
    let (s, v) = t.call(Method::GET, &format!("/channels/{}/messages", dm.id), Some(&b), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 1);

    // непрочитанные: у b — 1, у автора — нет
    assert_eq!(unread(&t, &b).await, vec![UnreadChannel { channel_id: dm.id, server_id: None, count: 1, last_message_id: msg.id }]);
    assert!(unread(&t, &a).await.is_empty());
    let (s, _) = t.call(Method::POST, &format!("/channels/{}/read", dm.id), Some(&b), Some(json!({"message_id": msg.id}))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(sb.next_where(|e| matches!(e, ServerEvent::ReadState { .. })).await, ServerEvent::ReadState { channel_id: dm.id, last_read_id: msg.id });
    assert!(unread(&t, &b).await.is_empty());

    // третий не видит ни историю, ни отправку, ни «прочитано»
    let (s, _) = t.call(Method::GET, &format!("/channels/{}/messages", dm.id), Some(&c), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = send(&t, &c, dm.id, "эй").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = t.call(Method::POST, &format!("/channels/{}/read", dm.id), Some(&c), Some(json!({"message_id": 1}))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // канал ЛС не трогается ручками серверных каналов
    let (s, _) = t.call(Method::POST, &format!("/channels/{}/join", dm.id), Some(&a), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // удалили из друзей — история читается, писать нельзя
    let (s, _) = t.call(Method::DELETE, &format!("/friends/{b_id}"), Some(&a), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = send(&t, &b, dm.id, "ну и ладно").await;
    assert_eq!((s, err(&v).as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "validation"));
    let (s, _) = t.call(Method::GET, &format!("/channels/{}/messages", dm.id), Some(&b), None).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn dm_policy_for_server_neighbours() {
    let t = setup().await;
    let (a, a_id) = t.user("okabe").await;
    let (b, b_id) = t.user("kurisu").await;
    let (_, c_id) = t.user("faris").await;

    // чужой человек без общих серверов — нельзя
    let (s, v) = open_dm(&t, &a, c_id).await;
    assert_eq!((s, err(&v).as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "validation"));
    let (s, _) = open_dm(&t, &a, a_id).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "самому себе");
    let (s, _) = open_dm(&t, &a, 9999).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // общий сервер, но b принимает ЛС только от друзей
    let srv = t.create_server(&a, "Лаборатория").await;
    t.add_member(srv.id, b_id, 0).await;
    let (s, _) = open_dm(&t, &a, b_id).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // b разрешил ЛС с общих серверов
    let (s, v) = t.call(Method::PATCH, "/me", Some(&b), Some(json!({"dm_from_servers": true}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["dm_from_servers"], true);
    let (s, v) = open_dm(&t, &a, b_id).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let dm: DmChannel = serde_json::from_value(v).unwrap();
    assert_eq!(send(&t, &a, dm.id, "привет").await.0, StatusCode::CREATED);
    // a принимает только от друзей, но диалог начал сам — b может ответить
    assert_eq!(send(&t, &b, dm.id, "привет-привет").await.0, StatusCode::CREATED);
    // b передумал — a больше писать не может
    t.call(Method::PATCH, "/me", Some(&b), Some(json!({"dm_from_servers": false}))).await;
    assert_eq!(send(&t, &a, dm.id, "ещё").await.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn unread_in_text_channels() {
    let t = setup().await;
    let (a, _) = t.user("okabe").await;
    let (b, b_id) = t.user("kurisu").await;
    let srv = t.create_server(&a, "Лаборатория").await;
    t.add_member(srv.id, b_id, 0).await;
    let (s, v) = t.call(Method::POST, &format!("/servers/{}/channels", srv.id), Some(&a), Some(json!({"name": "чат", "kind": "text"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let ch = v["id"].as_i64().unwrap();

    let m1: Message = serde_json::from_value(send(&t, &a, ch, "раз").await.1).unwrap();
    let m2: Message = serde_json::from_value(send(&t, &a, ch, "два").await.1).unwrap();
    assert!(unread(&t, &a).await.is_empty(), "свои не считаются");
    assert_eq!(unread(&t, &b).await, vec![UnreadChannel { channel_id: ch, server_id: Some(srv.id), count: 2, last_message_id: m2.id }]);

    // прочитал первое — осталось одно; назад не откатывается
    t.call(Method::POST, &format!("/channels/{ch}/read"), Some(&b), Some(json!({"message_id": m1.id}))).await;
    assert_eq!(unread(&t, &b).await[0].count, 1);
    t.call(Method::POST, &format!("/channels/{ch}/read"), Some(&b), Some(json!({"message_id": 0}))).await;
    assert_eq!(unread(&t, &b).await[0].count, 1);
    // ответил сам — всё до своего сообщения прочитано
    send(&t, &b, ch, "три").await;
    assert!(unread(&t, &b).await.is_empty());
}

#[tokio::test]
async fn dm_call_ring_answer_end() {
    let t = setup().await;
    let (a, a_id) = t.user("okabe").await;
    let (b, b_id) = t.user("kurisu").await;
    befriend(&t, &a, "kurisu", a_id, &b).await;
    let dm: DmChannel = serde_json::from_value(open_dm(&t, &a, b_id).await.1).unwrap();
    let mut sa = Stream::open(&t, &a).await;
    let mut sb = Stream::open(&t, &b).await;

    // a звонит
    let (s, v) = t.call(Method::POST, &format!("/dms/{}/call", dm.id), Some(&a), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["room"], format!("dm-{}", dm.id));
    let call = sb.next_call().await.expect("звонок");
    assert!(call.ringing);
    assert_eq!(call.caller_id, a_id);
    assert!(sa.next_call().await.unwrap().ringing);
    t.set_voice(&format!("dm-{}", dm.id), &[a_id]).await;
    let call = sb.next_call().await.unwrap();
    assert_eq!((call.ringing, call.participants.clone()), (true, vec![a_id]));
    let lb = dms(&t, &b).await;
    assert_eq!(lb.len(), 1, "пустой диалог виден b, пока в нём звонят");
    assert!(lb[0].call.as_ref().is_some_and(|c| c.ringing));
    // b отвечает
    let (s, _) = t.call(Method::POST, &format!("/dms/{}/call", dm.id), Some(&b), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!sb.next_call().await.unwrap().ringing);
    t.set_voice(&format!("dm-{}", dm.id), &[a_id, b_id]).await;
    assert_eq!(sb.next_call().await.unwrap().participants, vec![a_id, b_id]);
    // оба вышли — звонка нет
    t.set_voice(&format!("dm-{}", dm.id), &[]).await;
    assert_eq!(sb.next_call().await, None);
    assert_eq!(dms(&t, &a).await[0].call, None);
    assert!(dms(&t, &b).await.is_empty(), "звонок кончился — пустой диалог снова не виден b");

    // a звонит снова, b отклоняет
    t.call(Method::POST, &format!("/dms/{}/call", dm.id), Some(&a), None).await;
    assert!(sb.next_call().await.unwrap().ringing);
    let (s, _) = t.call(Method::DELETE, &format!("/dms/{}/call", dm.id), Some(&b), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(sa.next_where(|e| matches!(e, ServerEvent::Call { call: None, .. })).await, ServerEvent::Call { channel_id: dm.id, call: None });

    // посторонний позвонить не может
    let (c, _) = t.user("daru").await;
    let (s, _) = t.call(Method::POST, &format!("/dms/{}/call", dm.id), Some(&c), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
