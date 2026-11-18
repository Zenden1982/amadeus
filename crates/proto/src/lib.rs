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
    /// Текст сообщения, символов.
    pub const MESSAGE_MAX: usize = 4000;
    /// Вложений в одном сообщении.
    pub const ATTACHMENTS_MAX: usize = 4;
    /// Размер одной картинки, байт.
    pub const IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;
    /// Сторона картинки, пикселей.
    pub const IMAGE_MAX_SIDE: u32 = 16_384;
    /// Сообщений в одной странице истории (по умолчанию / максимум).
    pub const HISTORY_PAGE: u32 = 50;
    pub const HISTORY_PAGE_MAX: u32 = 100;
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

/// Автор сообщения (минимум, чтобы нарисовать строку).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub id: i64,
    pub name: String,
}

/// Вложение (пока только картинки). Файл — `GET /attachments/{id}` с авторизацией.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i64,
    /// `image/png`, `image/jpeg`, `image/gif`, `image/webp`.
    pub mime: String,
    pub size: u64,
    pub width: u32,
    pub height: u32,
}

/// Сообщение текстового канала. Время — unix-секунды.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub channel_id: i64,
    pub author: Author,
    pub content: String,
    pub created_at: i64,
    /// Задел на правку (чат, вариант B): когда сообщение правили.
    #[serde(default)]
    pub edited_at: Option<i64>,
    /// Задел на ответы (вариант B): id сообщения, на которое отвечают.
    #[serde(default)]
    pub reply_to: Option<i64>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// `POST /channels/{id}/messages`. Вложения сначала загружаются через `POST /uploads`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendMessageRequest {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<i64>,
}

/// Событие потока `GET /events` (SSE, поле `data` — этот JSON). Незнакомые `type` клиент пропускает.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// Поток открыт. После переподключения клиент дозапрашивает пропущенное (`?after=`).
    Hello,
    /// Сервер не успел доставить часть событий — перечитать открытые каналы.
    Resync,
    MessageCreated { message: Message },
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
    fn server_event_tagged() {
        let s = serde_json::to_string(&ServerEvent::Hello).unwrap();
        assert_eq!(s, r#"{"type":"hello"}"#);
        let e: ServerEvent = serde_json::from_str(
            r#"{"type":"message_created","message":{"id":1,"channel_id":2,"author":{"id":3,"name":"a"},"content":"hi","created_at":5}}"#,
        )
        .unwrap();
        assert!(matches!(e, ServerEvent::MessageCreated { message } if message.attachments.is_empty()));
        assert!(serde_json::from_str::<ServerEvent>(r#"{"type":"future_thing"}"#).is_err());
    }

    #[test]
    fn create_invite_defaults() {
        let r: CreateInviteRequest = serde_json::from_str("{}").unwrap();
        assert!(r.uses.is_none() && r.expires_in_hours.is_none());
    }
}
