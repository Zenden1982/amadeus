//! Бэкенд Amadeus: аккаунты (только по инвайтам), каналы, выдача LiveKit-токенов.

pub mod config;
pub mod error;
pub mod proxy_protocol;
pub mod ratelimit;
pub mod security;

mod db;
mod extract;

pub use extract::ClientAddr;
mod routes;

use std::{sync::Arc, time::Duration};

pub use db::{backup, connect, create_invite, create_user};
pub use routes::router;

use config::LiveKitConfig;
use ratelimit::RateLimiter;

/// Срок жизни сессии клиента.
pub const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
/// Срок жизни LiveKit-токена (PLAN.md §3, фаза 2).
pub const LIVEKIT_TOKEN_TTL: Duration = Duration::from_secs(6 * 3600);
/// Лимит попыток входа/регистрации с одного IP.
pub const AUTH_ATTEMPTS: u32 = 10;
pub const AUTH_WINDOW: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

pub struct Inner {
    pub db: sqlx::SqlitePool,
    pub livekit: LiveKitConfig,
    pub trusted_proxy_hops: usize,
    pub auth_limiter: RateLimiter,
}

impl AppState {
    pub fn new(db: sqlx::SqlitePool, livekit: LiveKitConfig, trusted_proxy_hops: usize) -> Self {
        Self::with_limiter(
            db,
            livekit,
            trusted_proxy_hops,
            RateLimiter::new(AUTH_ATTEMPTS, AUTH_WINDOW),
        )
    }

    pub fn with_limiter(
        db: sqlx::SqlitePool,
        livekit: LiveKitConfig,
        trusted_proxy_hops: usize,
        auth_limiter: RateLimiter,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                livekit,
                trusted_proxy_hops,
                auth_limiter,
            }),
        }
    }
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
