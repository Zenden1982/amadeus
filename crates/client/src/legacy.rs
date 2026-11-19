//! Переход со старой установки Amadeus на Murmur (редизайн, задача 12).
//! Старые клиенты 0.2.x автообновлением подменяют только exe — программа оказывается в папке Amadeus, а ярлык и запись
//! в «Приложениях» остаются старыми. Такой запуск распознаём и предлагаем поставить установщик Murmur
//! (он сам убирает старую установку — preinstall-секция в crates/client/Cargo.toml).

use std::path::Path;

/// Запущены из старой установки: exe лежит в `InstallLocation` записи Amadeus, а записи Murmur ещё нет.
pub fn is_legacy_install(exe: &Path, old_install_location: Option<&Path>, new_uninstall_present: bool) -> bool {
    if new_uninstall_present {
        return false;
    }
    let Some(old) = old_install_location else { return false };
    let norm = |p: &Path| p.to_string_lossy().trim_matches('"').trim_end_matches(['\\', '/']).to_lowercase();
    exe.parent().is_some_and(|dir| norm(dir) == norm(old))
}

const OLD_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Amadeus";
const NEW_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Murmur";

/// Запущены из старой установки Amadeus (см. `is_legacy_install`).
pub fn detect() -> bool {
    let Some(exe) = crate::desktop::current_exe() else { return false };
    let old = crate::desktop::hkcu_string(OLD_KEY, "InstallLocation").map(std::path::PathBuf::from);
    let new_present = crate::desktop::hkcu_string(NEW_KEY, "DisplayName").is_some();
    let legacy = is_legacy_install(&exe, old.as_deref(), new_present);
    if legacy {
        log::info!("запуск из старой установки Amadeus ({}) — предлагаю установщик Murmur", exe.display());
    }
    legacy
}

/// Скачать установщик (`{api}/get`) во временную папку и вернуть путь к нему.
pub async fn download_installer(api_base: String) -> Result<std::path::PathBuf, String> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("Murmur/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = http.get(format!("{api_base}/get")).send().await.map_err(|e| format!("нет связи с сервером: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("сервер ответил {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("загрузка прервалась: {e}"))?;
    let path = std::env::temp_dir().join("murmur-setup.exe");
    std::fs::write(&path, &bytes).map_err(|e| format!("не удалось сохранить: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn legacy_detection() {
        let old = PathBuf::from(r"C:\Users\u\AppData\Local\Amadeus");
        let exe = PathBuf::from(r"C:\Users\u\AppData\Local\Amadeus\amadeus.exe");
        assert!(is_legacy_install(&exe, Some(&old), false));
        // запись в реестре — в кавычках и другим регистром
        let quoted = PathBuf::from(r#""c:\users\u\appdata\local\amadeus""#);
        assert!(is_legacy_install(&exe, Some(&quoted), false));
        assert!(!is_legacy_install(&exe, Some(&old), true), "Murmur уже установлен");
        let dev = PathBuf::from(r"C:\src\discord\target\release\amadeus.exe");
        assert!(!is_legacy_install(&dev, Some(&old), false), "сборка из target — не старая установка");
        assert!(!is_legacy_install(&exe, None, false), "Amadeus не устанавливали");
    }
}
