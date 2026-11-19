//! Набор деталей интерфейса в стиле A «мягкий» (спека docs/superpowers/specs/2026-11-19-redesign-murmur-design.md).
//! Цвета — только из темы (`t::p()`), размеры — из токенов ниже.

// Часть деталей экраны начнут использовать в задачах 7–11 (перевод экранов).
#![allow(dead_code, unused_imports)]

mod buttons;
mod inputs;
mod lists;

pub use buttons::*;
pub use inputs::*;
pub use lists::*;

/// Скругление панелей, карточек, плиток.
pub const R_CARD: u8 = 14;
/// Скругление строк списков.
pub const R_ROW: u8 = 10;
/// Скругление полей ввода.
pub const R_FIELD: u8 = 12;
/// «Таблетка» — полное скругление.
pub const R_PILL: u8 = 255;
/// Внутренний отступ (×1.15 к прежним 8).
pub const PAD: f32 = 9.2;
/// Промежуток между элементами.
pub const GAP: f32 = 9.2;
/// Длительность анимации наведения, с.
pub const HOVER_S: f32 = 0.12;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_scaled() {
        const { assert!(PAD > 8.0 && GAP > 8.0, "плотность просторнее прежней (8.0)") };
        assert_eq!(R_PILL, 255);
        assert_eq!((R_CARD, R_ROW, R_FIELD), (14, 10, 12));
    }
}
