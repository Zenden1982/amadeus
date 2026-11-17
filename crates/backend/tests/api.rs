//! Интеграционные тесты HTTP API (in-memory SQLite, без сети).

use std::time::Duration;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use backend::{AppState, config::LiveKitConfig, ratelimit::RateLimiter};
use http_body_util::BodyExt;
use livekit_api::access_token::TokenVerifier;
use proto::{AuthResponse, Channel, ErrorBody, Invite, JoinResponse, User};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceExt;

const LK_KEY: &str = "APItestkey";
const LK_SECRET: &str = "test-secret-test-secret-test-secret-0123";
const LK_URL: &str = "wss://lk.example.test";

struct T {
    app: Router,
    pool: sqlx::SqlitePool,
}

async fn setup_with_limit(limit: u32) -> T {
    let opts: SqliteConnectOptions = "sqlite::memory:".parse().unwrap();
    // одно соединение — одна in-memory база на тест
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts.foreign_keys(true))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let lk = LiveKitConfig {
        public_url: LK_URL.into(),
        api_key: LK_KEY.into(),
        api_secret: LK_SECRET.into(),
    };
    let state = AppState::with_limiter(
        pool.clone(),
        lk,
        1,
        RateLimiter::new(limit, Duration::from_secs(300)),
    );
    T {
        app: backend::router(state),
        pool,
    }
}

async fn setup() -> T {
    setup_with_limit(1000).await
}

impl T {
    async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.call_from(method, uri, token, body, "203.0.113.1")
            .await
    }

    async fn call_from(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
        ip: &str,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-forwarded-for", ip);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let v = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, v)
    }

    async fn admin(&self) -> String {
        backend::create_user(&self.pool, "admin", "admin-password", true)
            .await
            .unwrap();
        self.login("admin", "admin-password").await
    }

    async fn login(&self, name: &str, password: &str) -> String {
        let (s, v) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({"name": name, "password": password})),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        serde_json::from_value::<AuthResponse>(v).unwrap().token
    }

    async fn invite(&self, admin: &str, body: Value) -> Invite {
        let (s, v) = self
            .call(Method::POST, "/invites", Some(admin), Some(body))
            .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        serde_json::from_value(v).unwrap()
    }

    async fn register(&self, code: &str, name: &str) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "/auth/register",
            None,
            Some(json!({"invite_code": code, "name": name, "password": "password123"})),
        )
        .await
    }
}

fn err(v: &Value) -> String {
    serde_json::from_value::<ErrorBody>(v.clone())
        .map(|e| e.error)
        .unwrap_or_default()
}

#[tokio::test]
async fn health() {
    let t = setup().await;
    let (s, v) = t.call(Method::GET, "/health", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["ok"], true);
}

#[tokio::test]
async fn register_requires_valid_invite() {
    let t = setup().await;
    let admin = t.admin().await;

    let (s, v) = t.register("NOPE-NOPE-NOPE", "vasya").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err(&v), "invalid_invite");

    let inv = t.invite(&admin, json!({})).await;
    assert_eq!(inv.uses_left, 1);
    // регистр и дефисы не важны
    let (s, v) = t
        .register(&inv.code.to_lowercase().replace('-', ""), "vasya")
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let auth: AuthResponse = serde_json::from_value(v).unwrap();
    assert_eq!(auth.user.name, "vasya");
    assert!(!auth.user.is_admin);

    // одноразовый инвайт исчерпан
    let (s, v) = t.register(&inv.code, "petya").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err(&v), "invalid_invite");

    // сессия из регистрации рабочая
    let (s, v) = t.call(Method::GET, "/me", Some(&auth.token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(serde_json::from_value::<User>(v).unwrap().name, "vasya");
}

#[tokio::test]
async fn multi_use_revoked_and_expired_invites() {
    let t = setup().await;
    let admin = t.admin().await;

    let inv = t.invite(&admin, json!({"uses": 2})).await;
    assert_eq!(t.register(&inv.code, "user1").await.0, StatusCode::CREATED);
    assert_eq!(t.register(&inv.code, "user2").await.0, StatusCode::CREATED);
    assert_eq!(
        t.register(&inv.code, "user3").await.0,
        StatusCode::BAD_REQUEST
    );

    let inv = t.invite(&admin, json!({"uses": 5})).await;
    let (s, _) = t
        .call(
            Method::DELETE,
            &format!("/invites/{}", inv.code),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        t.register(&inv.code, "user4").await.0,
        StatusCode::BAD_REQUEST
    );

    let inv = t.invite(&admin, json!({"uses": 5})).await;
    sqlx::query("UPDATE invites SET expires_at = 1 WHERE code = ?")
        .bind(&inv.code)
        .execute(&t.pool)
        .await
        .unwrap();
    assert_eq!(
        t.register(&inv.code, "user5").await.0,
        StatusCode::BAD_REQUEST
    );

    // бессрочный
    let inv = t.invite(&admin, json!({"expires_in_hours": 0})).await;
    assert!(inv.expires_at.is_none());
    assert_eq!(t.register(&inv.code, "user6").await.0, StatusCode::CREATED);

    let (s, v) = t.call(Method::GET, "/invites", Some(&admin), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn name_taken_does_not_burn_invite() {
    let t = setup().await;
    let admin = t.admin().await;
    let inv = t.invite(&admin, json!({})).await;
    let (s, v) = t.register(&inv.code, "ADMIN").await; // без учёта регистра
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(err(&v), "name_taken");
    // транзакция откатилась — инвайт ещё жив
    assert_eq!(t.register(&inv.code, "vasya").await.0, StatusCode::CREATED);
}

#[tokio::test]
async fn validation_errors() {
    let t = setup().await;
    let admin = t.admin().await;
    let inv = t.invite(&admin, json!({})).await;
    let (s, v) = t
        .call(
            Method::POST,
            "/auth/register",
            None,
            Some(json!({"invite_code": inv.code, "name": "ok_name", "password": "short"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err(&v), "validation");
    let (s, v) = t
        .call(Method::POST, "/auth/login", None, Some(json!({"name": 1})))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(err(&v), "bad_request");
    let (s, _) = t
        .call(
            Method::POST,
            "/invites",
            Some(&admin),
            Some(json!({"uses": 0})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn login_and_logout() {
    let t = setup().await;
    t.admin().await;
    let (s, v) = t
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({"name": "admin", "password": "wrong-password"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(err(&v), "bad_credentials");
    let (s, v) = t
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({"name": "ghost", "password": "whatever1"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(err(&v), "bad_credentials");

    let tok = t.login("Admin", "admin-password").await; // имя без учёта регистра
    assert_eq!(
        t.call(Method::GET, "/me", Some(&tok), None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        t.call(Method::POST, "/auth/logout", Some(&tok), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.call(Method::GET, "/me", Some(&tok), None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        t.call(Method::GET, "/me", Some("garbage"), None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        t.call(Method::GET, "/channels", None, None).await.0,
        StatusCode::UNAUTHORIZED
    );

    // сессии хранятся только хэшем
    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_all(&t.pool)
        .await
        .unwrap();
    assert!(!stored.contains(&tok));
}

#[tokio::test]
async fn expired_session_rejected() {
    let t = setup().await;
    let tok = t.admin().await;
    sqlx::query("UPDATE sessions SET expires_at = 1")
        .execute(&t.pool)
        .await
        .unwrap();
    assert_eq!(
        t.call(Method::GET, "/me", Some(&tok), None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn login_rate_limited_per_ip() {
    let t = setup_with_limit(3).await;
    t.admin().await; // 1 попытка с 203.0.113.1
    for _ in 0..2 {
        let (s, _) = t
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({"name": "admin", "password": "bad-password"})),
            )
            .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    let (s, v) = t
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({"name": "admin", "password": "admin-password"})),
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(err(&v), "rate_limited");
    // другой IP (из X-Forwarded-For от доверенного прокси) — не заблокирован;
    // подделанное начало заголовка не помогает обойти лимит
    let (s, _) = t
        .call_from(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({"name": "admin", "password": "admin-password"})),
            "198.51.100.7",
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = t
        .call_from(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({"name": "admin", "password": "admin-password"})),
            "198.51.100.9, 203.0.113.1",
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn channels_admin_only() {
    let t = setup().await;
    let admin = t.admin().await;
    let inv = t.invite(&admin, json!({})).await;
    let user: AuthResponse =
        serde_json::from_value(t.register(&inv.code, "vasya").await.1).unwrap();

    let (s, _) = t
        .call(
            Method::POST,
            "/channels",
            Some(&user.token),
            Some(json!({"name": "x", "kind": "voice"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = t
        .call(Method::POST, "/invites", Some(&user.token), Some(json!({})))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(
        t.call(Method::GET, "/invites", Some(&user.token), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let (s, v) = t
        .call(
            Method::POST,
            "/channels",
            Some(&admin),
            Some(json!({"name": " Общий ", "kind": "voice"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let ch: Channel = serde_json::from_value(v).unwrap();
    assert_eq!(ch.name, "Общий");
    t.call(
        Method::POST,
        "/channels",
        Some(&admin),
        Some(json!({"name": "чат", "kind": "text"})),
    )
    .await;

    let (s, v) = t
        .call(Method::GET, "/channels", Some(&user.token), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let list: Vec<Channel> = serde_json::from_value(v).unwrap();
    assert_eq!(list.len(), 2);

    assert_eq!(
        t.call(
            Method::DELETE,
            &format!("/channels/{}", ch.id),
            Some(&user.token),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.call(
            Method::DELETE,
            &format!("/channels/{}", ch.id),
            Some(&admin),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.call(
            Method::DELETE,
            &format!("/channels/{}", ch.id),
            Some(&admin),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn join_issues_valid_livekit_token() {
    let t = setup().await;
    let admin = t.admin().await;
    let inv = t.invite(&admin, json!({})).await;
    let user: AuthResponse = serde_json::from_value(t.register(&inv.code, "Вася").await.1).unwrap();
    let (_, v) = t
        .call(
            Method::POST,
            "/channels",
            Some(&admin),
            Some(json!({"name": "Голос", "kind": "voice"})),
        )
        .await;
    let voice: Channel = serde_json::from_value(v).unwrap();
    let (_, v) = t
        .call(
            Method::POST,
            "/channels",
            Some(&admin),
            Some(json!({"name": "чат", "kind": "text"})),
        )
        .await;
    let text: Channel = serde_json::from_value(v).unwrap();

    assert_eq!(
        t.call(
            Method::POST,
            &format!("/channels/{}/join", voice.id),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        t.call(Method::POST, "/channels/999/join", Some(&user.token), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.call(
            Method::POST,
            &format!("/channels/{}/join", text.id),
            Some(&user.token),
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    let (s, v) = t
        .call(
            Method::POST,
            &format!("/channels/{}/join", voice.id),
            Some(&user.token),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let j: JoinResponse = serde_json::from_value(v).unwrap();
    assert_eq!(j.livekit_url, LK_URL);

    let claims = TokenVerifier::with_api_key(LK_KEY, LK_SECRET)
        .verify(&j.livekit_token)
        .expect("valid jwt");
    assert_eq!(claims.sub, user.user.id.to_string());
    assert_eq!(claims.name, "Вася");
    assert_eq!(claims.iss, LK_KEY);
    assert!(claims.video.room_join);
    assert_eq!(claims.video.room, j.room);
    assert_eq!(claims.video.room, format!("ch-{}", voice.id));
    assert!(claims.video.can_publish() && claims.video.can_subscribe());
    assert_eq!(
        claims.video.can_publish_sources,
        ["microphone", "screen_share", "screen_share_audio"]
    );
    assert!(!claims.video.can_update_own_metadata());
    assert!(!claims.video.room_admin && !claims.video.room_create);
    let ttl = claims.exp as i64 - claims.nbf as i64;
    assert!((6 * 3600 - 5..=6 * 3600 + 5).contains(&ttl), "ttl {ttl}");

    // чужим секретом не проверяется
    assert!(
        TokenVerifier::with_api_key(LK_KEY, "other-secret-other-secret-other-secret")
            .verify(&j.livekit_token)
            .is_err()
    );
}

#[tokio::test]
async fn backup_makes_consistent_copy() {
    let dir = std::env::temp_dir().join(format!("amadeus-backup-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let src = dir.join("src.db");
    let url = format!("sqlite://{}", src.to_str().unwrap().replace('\\', "/"));
    let pool = backend::connect(&url).await.unwrap();
    backend::create_user(&pool, "alice", "password-123", true).await.unwrap();
    let out = dir.join("backup").join("app.db");
    backend::backup(&pool, &out).await.unwrap();
    // повторный бэкап заменяет файл
    backend::create_user(&pool, "bob", "password-123", false).await.unwrap();
    backend::backup(&pool, &out).await.unwrap();
    let copy_url = format!("sqlite://{}", out.to_str().unwrap().replace('\\', "/"));
    let copy = backend::connect(&copy_url).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&copy).await.unwrap();
    assert_eq!(n, 2);
    copy.close().await;
    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}
