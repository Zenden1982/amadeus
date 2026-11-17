//! Пароли (argon2id), случайные токены и инвайт-коды.

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// Хэш-пустышка: проверяем пароль против него, когда пользователя нет, —
/// чтобы время ответа не выдавало, существует ли имя.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash_password("dummy-password-for-timing").expect("argon2 dummy hash"));

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("argon2 hash: {e}"))?;
    Ok(hash.to_string())
}

/// Проверка пароля. `hash = None` — пользователя нет: тратим то же время и возвращаем false.
pub fn verify_password(password: &str, hash: Option<&str>) -> bool {
    match hash {
        Some(h) => Argon2::default()
            .verify_password(password.as_bytes(), h)
            .is_ok(),
        None => {
            let _ = Argon2::default().verify_password(password.as_bytes(), DUMMY_HASH.as_str());
            false
        }
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Новый сессионный токен (256 бит, hex).
pub fn new_session_token() -> String {
    let mut buf = [0u8; 32];
    rand::fill(&mut buf);
    to_hex(&buf)
}

/// sha256(token) в hex — то, что лежит в БД.
pub fn token_hash(token: &str) -> String {
    to_hex(&Sha256::digest(token.as_bytes()))
}

/// Алфавит инвайтов: без похожих символов (0/O, 1/I/L, U/V).
const INVITE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTWXYZ23456789";
const INVITE_LEN: usize = 12; // ~58 бит

/// Инвайт-код вида `ABCD-EFGH-JKMN`.
pub fn new_invite_code() -> String {
    let n = INVITE_ALPHABET.len() as u8;
    let limit = u8::MAX - (u8::MAX % n); // отбрасываем хвост — без смещения распределения
    let mut out = String::with_capacity(INVITE_LEN + 2);
    let mut count = 0;
    while count < INVITE_LEN {
        let mut buf = [0u8; 32];
        rand::fill(&mut buf);
        for b in buf {
            if b >= limit || count == INVITE_LEN {
                continue;
            }
            if count > 0 && count % 4 == 0 {
                out.push('-');
            }
            out.push(INVITE_ALPHABET[(b % n) as usize] as char);
            count += 1;
        }
    }
    out
}

/// Нормализация введённого кода: регистр, пробелы и дефисы не важны.
pub fn normalize_invite_code(input: &str) -> String {
    let clean: Vec<char> = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    clean
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse", Some(&h)));
        assert!(!verify_password("wrong", Some(&h)));
        assert!(!verify_password("correct horse", None));
    }

    #[test]
    fn tokens_are_random_and_hashed() {
        let a = new_session_token();
        let b = new_session_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(token_hash(&a).len(), 64);
        assert_ne!(token_hash(&a), a);
    }

    #[test]
    fn invite_code_format() {
        let c = new_invite_code();
        assert_eq!(c.len(), 14, "{c}");
        assert_eq!(c.matches('-').count(), 2);
        assert!(c.bytes().all(|b| b == b'-' || INVITE_ALPHABET.contains(&b)));
        assert_eq!(
            normalize_invite_code(&c.to_lowercase().replace('-', " ")),
            c
        );
    }
}
