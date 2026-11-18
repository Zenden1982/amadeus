//! Настройки клиента: %APPDATA%\\Amadeus\\settings.json.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::stream::Preset;

/// Адрес сервера по умолчанию (переопределяется при сборке: `AMADEUS_SERVER=https://… cargo build`).
pub const DEFAULT_SERVER: &str = match option_env!("AMADEUS_SERVER") {
    Some(s) => s,
    None => "https://api.steins-gate.ru",
};

/// Клавиша рации по умолчанию — V (виртуальный код Windows).
pub const DEFAULT_PTT_KEY: u32 = 0x56;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub server_url: String,
    /// Сессионный токен (лежит в профиле пользователя, как у браузера).
    pub session_token: Option<String>,
    pub last_name: String,
    /// GUID устройства WASAPI; None — системное по умолчанию.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub ptt_enabled: bool,
    pub ptt_key: u32,
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain: bool,
    pub sounds: bool,
    /// Громкость участников по id пользователя (identity), 1.0 = 100%.
    pub user_volume: HashMap<String, f32>,
    /// Заглушённые для себя участники.
    pub user_muted: HashMap<String, bool>,
    /// Качество трансляции по умолчанию (последнее выбранное).
    pub stream_preset: Preset,
    /// Передавать звук окна/системы в трансляции.
    pub stream_audio: bool,
    /// Громкость звука трансляций участников по identity (отдельно от голоса).
    pub stream_volume: HashMap<String, f32>,
    /// «Закрыть» прячет окно в трей (выход — меню значка в трее).
    pub close_to_tray: bool,
    /// Запускать вместе с Windows (только у установленной программы).
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER.to_string(),
            session_token: None,
            last_name: String::new(),
            input_device: None,
            output_device: None,
            ptt_enabled: false,
            ptt_key: DEFAULT_PTT_KEY,
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain: true,
            sounds: true,
            user_volume: HashMap::new(),
            user_muted: HashMap::new(),
            stream_preset: Preset::default(),
            stream_audio: true,
            stream_volume: HashMap::new(),
            close_to_tray: true,
            autostart: true,
        }
    }
}

/// Каталог настроек: `AMADEUS_CONFIG_DIR` (например, второй клиент на том же ПК) или %APPDATA%\\Amadeus.
fn path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("AMADEUS_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    directories::ProjectDirs::from("", "", "Amadeus").map(|d| d.config_dir().join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(p) = path() else { return Self::default() };
        match std::fs::read(&p) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                log::warn!("settings.json повреждён ({e}), беру настройки по умолчанию");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if std::fs::write(&tmp, bytes).is_ok() {
                    let _ = std::fs::rename(&tmp, &p);
                }
            }
            Err(e) => log::error!("не удалось сохранить настройки: {e}"),
        }
    }

    pub fn volume_of(&self, identity: &str) -> f32 {
        self.user_volume.get(identity).copied().unwrap_or(1.0)
    }

    pub fn stream_volume_of(&self, identity: &str) -> f32 {
        self.stream_volume.get(identity).copied().unwrap_or(1.0)
    }

    pub fn is_user_muted(&self, identity: &str) -> bool {
        self.user_muted.get(identity).copied().unwrap_or(false)
    }

    /// Итоговая громкость участника для WebRTC (с учётом «заглушить для себя»).
    pub fn effective_volume(&self, identity: &str) -> f32 {
        if self.is_user_muted(identity) { 0.0 } else { self.volume_of(identity) }
    }
}
