//! HTTP-клиент бэкенда Amadeus (типы — из `proto`).

use std::time::Duration;

use proto::{
    Attachment, AuthResponse, Channel, CreateInviteRequest, ErrorBody, Invite, JoinResponse, LoginRequest, Message,
    RegisterRequest, SendMessageRequest, User,
};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ApiError {
    /// Машинный код (`bad_credentials`, `name_taken`, …) или `network`.
    pub code: String,
    pub message: String,
    /// Исходный текст сервера (для `validation` — что именно не так).
    pub detail: String,
}

impl ApiError {
    fn network(e: reqwest::Error) -> Self {
        let message = if e.is_timeout() {
            "Сервер не ответил вовремя. Проверьте интернет.".to_string()
        } else if e.is_connect() {
            "Не удалось подключиться к серверу.".to_string()
        } else {
            format!("Ошибка сети: {e}")
        };
        Self { code: "network".into(), message, detail: String::new() }
    }

    pub fn is_unauthorized(&self) -> bool {
        self.code == "unauthorized"
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

pub type ApiResult<T> = Result<T, ApiError>;

/// Переводит ответ сервера с ошибкой в понятный текст (сервер шлёт английские сообщения).
fn humanize(status: StatusCode, body: Option<ErrorBody>) -> ApiError {
    let (code, server_msg) = match body {
        Some(b) => (b.error, b.message),
        None => (format!("http_{}", status.as_u16()), String::new()),
    };
    let message = match code.as_str() {
        "bad_credentials" => "Неверный ник или пароль.".to_string(),
        "invalid_invite" => "Приглашение недействительно: неверный код, истёк срок или уже использовано.".to_string(),
        "name_taken" => "Этот ник уже занят. Приглашение не потрачено — выберите другой.".to_string(),
        "rate_limited" => "Слишком много попыток. Подождите несколько минут.".to_string(),
        "unauthorized" => "Сессия истекла — войдите заново.".to_string(),
        "forbidden" => "Недостаточно прав.".to_string(),
        "not_found" => "Не найдено.".to_string(),
        "validation" => format!("Проверьте поля: {server_msg}"),
        _ if server_msg.is_empty() => format!("Ошибка сервера ({status})."),
        _ => format!("Ошибка сервера: {server_msg}"),
    };
    ApiError { code, message, detail: server_msg }
}

#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    /// Для долгих ответов (поток событий): без общего таймаута, но с таймаутом чтения.
    stream_http: reqwest::Client,
    base: String,
    token: Option<String>,
}

/// Поток событий SSE: сервер шлёт пинг каждые 15 с — тишина дольше значит обрыв.
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(40);
/// Загрузка картинки (до 8 МБ на медленном канале).
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(180);

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api").field("base", &self.base).field("authorized", &self.token.is_some()).finish()
    }
}

impl Api {
    pub fn new(base: &str, token: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("Amadeus/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client");
        let stream_http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .read_timeout(STREAM_READ_TIMEOUT)
            .user_agent(concat!("Amadeus/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client");
        Self { http, stream_http, base: base.trim_end_matches('/').to_string(), token }
    }

    pub fn with_token(&self, token: Option<String>) -> Self {
        Self { http: self.http.clone(), stream_http: self.stream_http.clone(), base: self.base.clone(), token }
    }

    /// Адрес сервера без «/» на конце.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Последняя выпущенная версия клиента; `None` — сервер ещё ничего не выпускал (404).
    pub async fn latest_release(&self) -> ApiResult<Option<proto::ReleaseInfo>> {
        let resp = self.request(Method::GET, "/version").send().await.map_err(ApiError::network)?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::decode(resp).await.map(Some)
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let req = self.http.request(method, format!("{}{}", self.base, path));
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    async fn call<B: Serialize, T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&B>) -> ApiResult<T> {
        let mut req = self.request(method, path);
        if let Some(b) = body {
            req = req.json(b);
        }
        Self::decode(req.send().await.map_err(ApiError::network)?).await
    }

    async fn decode<T: DeserializeOwned>(resp: reqwest::Response) -> ApiResult<T> {
        let status = resp.status();
        let bad = |e: &dyn std::fmt::Display| ApiError {
            code: "decode".into(),
            message: format!("Непонятный ответ сервера: {e}"),
            detail: String::new(),
        };
        if status == StatusCode::NO_CONTENT {
            serde_json::from_value(serde_json::Value::Null).map_err(|e| bad(&e))
        } else if status.is_success() {
            resp.json::<T>().await.map_err(|e| bad(&e))
        } else {
            let body = resp.json::<ErrorBody>().await.ok();
            Err(humanize(status, body))
        }
    }

    pub async fn login(&self, name: &str, password: &str) -> ApiResult<AuthResponse> {
        let body = LoginRequest { name: name.to_string(), password: password.to_string() };
        self.call(Method::POST, "/auth/login", Some(&body)).await
    }

    pub async fn register(&self, invite_code: &str, name: &str, password: &str) -> ApiResult<AuthResponse> {
        let body = RegisterRequest { invite_code: invite_code.to_string(), name: name.to_string(), password: password.to_string() };
        self.call(Method::POST, "/auth/register", Some(&body)).await
    }

    pub async fn logout(&self) -> ApiResult<()> {
        self.call::<(), _>(Method::POST, "/auth/logout", None).await
    }

    pub async fn me(&self) -> ApiResult<User> {
        self.call::<(), _>(Method::GET, "/me", None).await
    }

    pub async fn channels(&self) -> ApiResult<Vec<Channel>> {
        self.call::<(), _>(Method::GET, "/channels", None).await
    }

    pub async fn join(&self, channel_id: i64) -> ApiResult<JoinResponse> {
        self.call::<(), _>(Method::POST, &format!("/channels/{channel_id}/join"), None).await
    }

    pub async fn create_invite(&self) -> ApiResult<Invite> {
        self.call(Method::POST, "/invites", Some(&CreateInviteRequest::default())).await
    }

    /// Страница истории (по возрастанию id): последние, старше `before` или новее `after`.
    pub async fn messages(&self, channel_id: i64, before: Option<i64>, after: Option<i64>) -> ApiResult<Vec<Message>> {
        let mut path = format!("/channels/{channel_id}/messages?limit={}", proto::limits::HISTORY_PAGE);
        if let Some(b) = before {
            path.push_str(&format!("&before={b}"));
        }
        if let Some(a) = after {
            path.push_str(&format!("&after={a}"));
        }
        self.call::<(), _>(Method::GET, &path, None).await
    }

    pub async fn send_message(&self, channel_id: i64, req: &SendMessageRequest) -> ApiResult<Message> {
        self.call(Method::POST, &format!("/channels/{channel_id}/messages"), Some(req)).await
    }

    /// Загрузить картинку (тело — файл как есть). Id вложения указывается при отправке сообщения.
    pub async fn upload(&self, bytes: Vec<u8>) -> ApiResult<Attachment> {
        let req = self
            .request(Method::POST, "/uploads")
            .timeout(UPLOAD_TIMEOUT)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes);
        Self::decode(req.send().await.map_err(ApiError::network)?).await
    }

    /// Файл вложения.
    pub async fn attachment(&self, id: i64) -> ApiResult<Vec<u8>> {
        let resp = self.request(Method::GET, &format!("/attachments/{id}")).send().await.map_err(ApiError::network)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(humanize(status, resp.json::<ErrorBody>().await.ok()));
        }
        Ok(resp.bytes().await.map_err(ApiError::network)?.to_vec())
    }

    /// Открыть поток событий `GET /events` (SSE). Данные читать `Response::chunk()`.
    pub async fn events(&self) -> ApiResult<reqwest::Response> {
        let mut req = self
            .stream_http
            .get(format!("{}/events", self.base))
            .header(reqwest::header::ACCEPT, "text/event-stream");
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.map_err(ApiError::network)?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            Err(humanize(status, resp.json::<ErrorBody>().await.ok()))
        }
    }
}

/// Нормализует код приглашения: только буквы/цифры в верхнем регистре, максимум 12 знаков.
pub fn normalize_invite(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).take(12).collect()
}

/// `K7PMQ4XXYYZZ` → `K7PM-Q4XX-YYZZ`.
pub fn format_invite(norm: &str) -> String {
    norm.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap_or("")).collect::<Vec<_>>().join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_normalization() {
        assert_eq!(normalize_invite(" k7pm-q4xx-yyzz "), "K7PMQ4XXYYZZ");
        assert_eq!(normalize_invite("k7pm q4xx yyzz extra"), "K7PMQ4XXYYZZ");
        assert_eq!(format_invite("K7PMQ4XXYYZZ"), "K7PM-Q4XX-YYZZ");
        assert_eq!(format_invite("K7PMQ"), "K7PM-Q");
    }

    #[test]
    fn humanize_known_codes() {
        let e = humanize(StatusCode::CONFLICT, Some(ErrorBody { error: "name_taken".into(), message: "x".into() }));
        assert!(e.message.contains("не потрачено"));
        let e = humanize(StatusCode::BAD_GATEWAY, None);
        assert_eq!(e.code, "http_502");
    }
}
