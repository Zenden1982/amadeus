use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use proto::ErrorBody;

#[derive(Debug)]
pub enum AppError {
    Unauthorized,
    Forbidden,
    NotFound,
    BadCredentials,
    InvalidInvite,
    /// Забанен на этом сервере.
    Banned,
    /// Голосовой канал заполнен (лимит).
    ChannelFull,
    NameTaken,
    Validation(String),
    BadRequest(String),
    RateLimited { retry_after_secs: u64 },
    Internal(anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

macro_rules! internal_from {
    ($($t:ty),*) => {$(
        impl From<$t> for AppError {
            fn from(e: $t) -> Self {
                AppError::Internal(e.into())
            }
        }
    )*};
}
internal_from!(
    anyhow::Error,
    sqlx::Error,
    tokio::task::JoinError,
    livekit_api::access_token::AccessTokenError
);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "нужна авторизация".into(),
            ),
            AppError::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "недостаточно прав".into(),
            ),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", "не найдено".into()),
            AppError::BadCredentials => (
                StatusCode::UNAUTHORIZED,
                "bad_credentials",
                "неверное имя или пароль".into(),
            ),
            AppError::InvalidInvite => (
                StatusCode::BAD_REQUEST,
                "invalid_invite",
                "инвайт-код недействителен, истёк или исчерпан".into(),
            ),
            AppError::Banned => (StatusCode::FORBIDDEN, "banned", "вы забанены на этом сервере".into()),
            AppError::ChannelFull => (StatusCode::CONFLICT, "channel_full", "канал заполнен".into()),
            AppError::NameTaken => (
                StatusCode::CONFLICT,
                "name_taken",
                "это имя уже занято".into(),
            ),
            AppError::Validation(m) => (StatusCode::UNPROCESSABLE_ENTITY, "validation", m.clone()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            AppError::RateLimited { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "слишком много попыток, подождите".into(),
            ),
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "внутренняя ошибка сервера".into(),
                )
            }
        };
        let mut resp = (
            status,
            Json(ErrorBody {
                error: code.into(),
                message,
            }),
        )
            .into_response();
        if let AppError::RateLimited { retry_after_secs } = self {
            resp.headers_mut()
                .insert(axum::http::header::RETRY_AFTER, retry_after_secs.into());
        }
        resp
    }
}
