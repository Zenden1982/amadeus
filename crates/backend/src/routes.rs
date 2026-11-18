use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use livekit_api::access_token::{AccessToken, VideoGrants};
use proto::{
    AuthResponse, Channel, ChannelKind, CreateChannelRequest, CreateInviteRequest, HealthResponse,
    Invite, JoinResponse, LoginRequest, RegisterRequest, User, limits,
};
use tower_http::{services::ServeDir, trace::TraceLayer};

use crate::{
    AppState, LIVEKIT_TOKEN_TTL, SESSION_TTL, chat, releases,
    db::{self, ChannelRow, InviteRow},
    error::{AppError, AppResult},
    extract::{ApiJson, AuthUser, ClientIp},
    now, security,
};

pub fn router(state: AppState) -> Router {
    let downloads = ServeDir::new(&state.inner.releases_dir).precompressed_gzip();
    Router::new()
        .route("/health", get(health))
        .route("/version", get(releases::version))
        .route("/get", get(releases::get_installer))
        .nest_service("/download", downloads)
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/me", get(me))
        .route("/channels", get(list_channels).post(create_channel))
        .route("/channels/{id}", delete(delete_channel))
        .route("/channels/{id}/join", post(join_channel))
        .route("/invites", get(list_invites).post(create_invite))
        .route("/invites/{code}", delete(revoke_invite))
        .route(
            "/channels/{id}/messages",
            get(chat::list_messages).post(chat::send_message),
        )
        .route(
            "/uploads",
            post(chat::upload).layer(DefaultBodyLimit::max(limits::IMAGE_MAX_BYTES + 64 * 1024)),
        )
        .route("/attachments/{id}", get(chat::get_attachment))
        .route("/events", get(chat::events))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").into(),
        protocol: proto::PROTOCOL_VERSION,
    })
}

async fn new_session(state: &AppState, user: User) -> AppResult<AuthResponse> {
    let token = security::new_session_token();
    let expires_at = now() + SESSION_TTL.as_secs() as i64;
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(security::token_hash(&token))
    .bind(user.id)
    .bind(now())
    .bind(expires_at)
    .execute(&state.inner.db)
    .await?;
    Ok(AuthResponse {
        token,
        expires_at,
        user,
    })
}

fn rate_limit(state: &AppState, ip: std::net::IpAddr) -> AppResult<()> {
    state
        .inner
        .auth_limiter
        .check(ip)
        .map_err(|retry_after_secs| {
            tracing::warn!(%ip, "auth rate limit");
            AppError::RateLimited { retry_after_secs }
        })
}

async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiJson(req): ApiJson<RegisterRequest>,
) -> AppResult<(StatusCode, Json<AuthResponse>)> {
    rate_limit(&state, ip)?;
    let name = db::validate_name(&req.name)?;
    db::validate_password(&req.password)?;
    let code = security::normalize_invite_code(&req.invite_code);
    let hash = db::hash_password_async(req.password).await?;

    // Списание инвайта и создание пользователя — одной транзакцией.
    let mut tx = state.inner.db.begin().await?;
    let used = sqlx::query(
        "UPDATE invites SET uses_left = uses_left - 1
         WHERE code = ? AND revoked = 0 AND uses_left > 0 AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(&code)
    .bind(now())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if used != 1 {
        return Err(AppError::InvalidInvite);
    }
    let id = db::insert_user(&mut tx, &name, &hash, false).await?;
    tx.commit().await?;
    tracing::info!(user_id = id, %name, "registered by invite");

    let resp = new_session(
        &state,
        User {
            display_name: None,
            avatar: None,
            id,
            name,
            is_admin: false,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiJson(req): ApiJson<LoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    rate_limit(&state, ip)?;
    let row: Option<(i64, String, String, bool)> =
        sqlx::query_as("SELECT id, name, password_hash, is_admin FROM users WHERE name = ?")
            .bind(req.name.trim())
            .fetch_optional(&state.inner.db)
            .await?;
    let hash = row.as_ref().map(|r| r.2.clone());
    let password = req.password;
    let ok =
        tokio::task::spawn_blocking(move || security::verify_password(&password, hash.as_deref()))
            .await?;
    let Some((id, name, _, is_admin)) = row.filter(|_| ok) else {
        return Err(AppError::BadCredentials);
    };
    sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(now())
        .execute(&state.inner.db)
        .await?;
    Ok(Json(
        new_session(&state, User { id, name, is_admin, display_name: None, avatar: None }).await?,
    ))
}

async fn logout(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(&user.token_hash)
        .execute(&state.inner.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(user: AuthUser) -> Json<User> {
    Json(User {
        display_name: None,
        avatar: None,
        id: user.id,
        name: user.name,
        is_admin: user.is_admin,
    })
}

async fn list_channels(
    State(state): State<AppState>,
    _user: AuthUser,
) -> AppResult<Json<Vec<Channel>>> {
    let rows: Vec<ChannelRow> = sqlx::query_as("SELECT id, name, kind, server_id, category_id, position, user_limit FROM channels ORDER BY id")
        .fetch_all(&state.inner.db)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(Channel::try_from)
            .collect::<AppResult<_>>()?,
    ))
}

async fn create_channel(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(req): ApiJson<CreateChannelRequest>,
) -> AppResult<(StatusCode, Json<Channel>)> {
    user.require_admin()?;
    let name = req.name.trim().to_string();
    if name.is_empty() || name.chars().count() > limits::CHANNEL_NAME_MAX {
        return Err(AppError::Validation(format!(
            "название: от 1 до {} символов",
            limits::CHANNEL_NAME_MAX
        )));
    }
    let id = sqlx::query("INSERT INTO channels (name, kind, created_at) VALUES (?, ?, ?)")
        .bind(&name)
        .bind(req.kind.as_str())
        .bind(now())
        .execute(&state.inner.db)
        .await?
        .last_insert_rowid();
    Ok((
        StatusCode::CREATED,
        Json(Channel {
            server_id: 1,
            category_id: None,
            position: 0,
            user_limit: None,
            id,
            name,
            kind: req.kind,
        }),
    ))
}

async fn delete_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    user.require_admin()?;
    // файлы вложений канала удаляются вслед за строками (каскад в БД)
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT a.storage_key FROM attachments a JOIN messages m ON m.id = a.message_id WHERE m.channel_id = ?",
    )
    .bind(id)
    .fetch_all(&state.inner.db)
    .await?;
    let n = sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(id)
        .execute(&state.inner.db)
        .await?
        .rows_affected();
    if n == 0 {
        Err(AppError::NotFound)
    } else {
        chat::remove_files(&state, &keys).await;
        Ok(StatusCode::NO_CONTENT)
    }
}

/// Имя комнаты LiveKit для канала.
pub fn room_name(channel_id: i64) -> String {
    format!("ch-{channel_id}")
}

async fn join_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<JoinResponse>> {
    let row: Option<ChannelRow> =
        sqlx::query_as(db::CHANNEL_SELECT_BY_ID)
            .bind(id)
            .fetch_optional(&state.inner.db)
            .await?;
    let channel = Channel::try_from(row.ok_or(AppError::NotFound)?)?;
    if channel.kind != ChannelKind::Voice {
        return Err(AppError::BadRequest(
            "в текстовый канал нельзя зайти голосом".into(),
        ));
    }
    let lk = &state.inner.livekit;
    let room = room_name(channel.id);
    let grants = VideoGrants {
        room_join: true,
        room: room.clone(),
        can_publish: Some(true),
        can_subscribe: Some(true),
        can_publish_data: Some(true),
        can_publish_sources: ["microphone", "screen_share", "screen_share_audio"]
            .map(String::from)
            .to_vec(),
        // имя задаёт только сервер (из токена) — участник не может переименоваться
        can_update_own_metadata: Some(false),
        ..Default::default()
    };
    let token = AccessToken::with_api_key(&lk.api_key, &lk.api_secret)
        .with_identity(&user.id.to_string())
        .with_name(&user.name)
        .with_grants(grants)
        .with_ttl(LIVEKIT_TOKEN_TTL)
        .to_jwt()?;
    tracing::info!(user_id = user.id, %room, "livekit token issued");
    Ok(Json(JoinResponse {
        livekit_url: lk.public_url.clone(),
        livekit_token: token,
        room,
    }))
}

async fn list_invites(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<Invite>>> {
    user.require_admin()?;
    let rows: Vec<InviteRow> = sqlx::query_as(
        "SELECT code, uses_left, expires_at, revoked, created_by FROM invites ORDER BY created_at DESC",
    )
    .fetch_all(&state.inner.db)
    .await?;
    Ok(Json(rows.into_iter().map(Invite::from).collect()))
}

async fn create_invite(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(req): ApiJson<CreateInviteRequest>,
) -> AppResult<(StatusCode, Json<Invite>)> {
    user.require_admin()?;
    let inv = db::create_invite(
        &state.inner.db,
        user.id,
        req.uses.unwrap_or(1),
        req.expires_in_hours.unwrap_or(168),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(inv)))
}

async fn revoke_invite(
    State(state): State<AppState>,
    user: AuthUser,
    Path(code): Path<String>,
) -> AppResult<StatusCode> {
    user.require_admin()?;
    let n = sqlx::query("UPDATE invites SET revoked = 1 WHERE code = ?")
        .bind(security::normalize_invite_code(&code))
        .execute(&state.inner.db)
        .await?
        .rows_affected();
    if n == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
