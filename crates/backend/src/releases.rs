//! Раздача клиента: `GET /version` (последний выпуск), `GET /download/{file}` (файлы из `RELEASES_DIR`),
//! `GET /get` → установщик. Без авторизации: ссылку на установщик можно просто отправить другу.
//!
//! Выпуск — это файлы в `RELEASES_DIR` и `latest.json` (`proto::ReleaseInfo`) рядом; кладёт их `scripts/release.sh`.
//! Для `amadeus-x.y.z.exe` можно положить `amadeus-x.y.z.exe.gz` — отдаётся клиентам, которые принимают gzip.

use axum::{
    Json,
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Redirect, Response},
};
use proto::ReleaseInfo;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

pub const LATEST: &str = "latest.json";

async fn latest(state: &AppState) -> AppResult<ReleaseInfo> {
    let path = state.inner.releases_dir.join(LATEST);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Internal(anyhow::anyhow!("чтение {LATEST}: {e}"))),
    };
    let info: ReleaseInfo = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{LATEST} повреждён: {e}")))?;
    if !info.is_valid() {
        return Err(AppError::Internal(anyhow::anyhow!("{LATEST}: недопустимые поля")));
    }
    Ok(info)
}

/// `GET /version`
pub async fn version(State(state): State<AppState>) -> AppResult<Response> {
    let mut resp = Json(latest(&state).await?).into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(resp)
}

/// `GET /get` — на установщик последней версии (относительная ссылка: работает за любым прокси).
pub async fn get_installer(State(state): State<AppState>) -> AppResult<Redirect> {
    let name = latest(&state).await?.installer.ok_or(AppError::NotFound)?;
    Ok(Redirect::temporary(&format!("download/{name}")))
}
