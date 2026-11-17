//! Общие типы HTTP API Amadeus (бэкенд ↔ клиент).
//!
//! Все тела запросов/ответов — JSON. Авторизация — заголовок `Authorization: Bearer <session token>`.

use serde::{Deserialize, Serialize};

/// Версия протокола API. Увеличивается при несовместимых изменениях.
pub const PROTOCOL_VERSION: u32 = 1;

/// Ограничения, общие для клиента и сервера (клиент может проверять заранее).
pub mod limits {
    pub const NAME_MIN: usize = 2;
    pub const NAME_MAX: usize = 32;
    pub const PASSWORD_MIN: usize = 8;
    pub const PASSWORD_MAX: usize = 128;
    pub const CHANNEL_NAME_MAX: usize = 64;
    pub const INVITE_USES_MAX: u32 = 100;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub protocol: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub invite_code: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub name: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    /// Сессионный токен (передавать как `Authorization: Bearer …`).
    pub token: String,
    /// Unix-время (секунды), когда сессия истекает.
    pub expires_at: i64,
    pub user: User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Voice,
    Text,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Voice => "voice",
            ChannelKind::Text => "text",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "voice" => Some(ChannelKind::Voice),
            "text" => Some(ChannelKind::Text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub id: i64,
    pub name: String,
    pub kind: ChannelKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub kind: ChannelKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResponse {
    pub livekit_url: String,
    pub livekit_token: String,
    /// Имя комнаты LiveKit (для отладки; клиенту знать не обязательно).
    pub room: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateInviteRequest {
    /// Сколько раз можно использовать (по умолчанию 1).
    #[serde(default)]
    pub uses: Option<u32>,
    /// Срок действия в часах (по умолчанию 168 = 7 дней; 0 — бессрочно).
    #[serde(default)]
    pub expires_in_hours: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub code: String,
    pub uses_left: u32,
    /// Unix-время истечения; None — бессрочно.
    pub expires_at: Option<i64>,
    pub revoked: bool,
    pub created_by: i64,
}

/// Тело любого ответа с ошибкой.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Машинный код: `unauthorized`, `forbidden`, `not_found`, `invalid_invite`, `name_taken`,
    /// `bad_credentials`, `validation`, `rate_limited`, `internal`, `bad_request`.
    pub error: String,
    /// Человекочитаемое описание.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_kind_serde_is_lowercase() {
        let c = Channel {
            id: 1,
            name: "Общий".into(),
            kind: ChannelKind::Voice,
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(s.contains(r#""kind":"voice""#), "{s}");
        assert_eq!(ChannelKind::parse("text"), Some(ChannelKind::Text));
        assert_eq!(ChannelKind::parse("x"), None);
    }

    #[test]
    fn create_invite_defaults() {
        let r: CreateInviteRequest = serde_json::from_str("{}").unwrap();
        assert!(r.uses.is_none() && r.expires_in_hours.is_none());
    }
}
