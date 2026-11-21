//! Кот в пустых и системных состояниях (пакет `murmur-ui-handoff/assets/cat-states`, viewBox 124×100, линия 4.5).
//! Пути SVG разбираются здесь же (M L H V Q T Z, относительные тоже) и рисуются кодом цветом акцента —
//! растеризатор SVG не нужен, а части (хвост, дуги, плюсики) можно анимировать по отдельности.

use std::time::Duration;

use eframe::egui::{self, Color32, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::{ButtonKind, button};
use crate::theme as t;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cat {
    /// Пустой голосовой канал.
    Sleep,
    /// Вход, первый запуск.
    Hello,
    /// Подключение (хвост-волна бежит, дуги у уха мерцают).
    Loading,
    /// Связь пропала.
    Offline,
    /// Приглашение не найдено.
    NotFound,
    /// Пустой текстовый канал (кот выглядывает).
    Peek,
    /// Успех (плюсики появляются один раз).
    Success,
    /// Нет микрофона.
    NoMic,
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Plain,
    /// Хвост-волна (Loading — анимируется).
    Tail,
    /// Дуги у уха (Loading — мерцают), номер дуги.
    Arc(u8),
    /// Плюсик (Success — появляется), центр в координатах макета.
    Plus(f32, f32),
}

struct Part {
    d: &'static str,
    width: f32,
    alpha: f32,
    role: Role,
}

const fn part(d: &'static str) -> Part {
    Part { d, width: 4.5, alpha: 1.0, role: Role::Plain }
}
const fn thin(d: &'static str, width: f32, alpha: f32, role: Role) -> Part {
    Part { d, width, alpha, role }
}

const BODY: &str = "M26 90 Q17 66 25 47 L30 22 L42 34 L72 34 L84 22 L89 47 Q97 66 88 90 Z";
const BODY_TALL: &str = "M26 90 Q17 66 25 47 L29 15 L42 34 L72 34 L85 15 L89 47 Q97 66 88 90 Z";
const TAIL_WAVE: &str = "M88 90 q5 -8 10 0 t10 0 t8 0";
const TAIL_CURL: &str = "M88 88 Q105 88 105 72 Q105 61 97 63";
const TAIL_FLAT: &str = "M88 90 L112 90";

/// Части кота и глаза-точки (x, y, r).
fn shape(cat: Cat) -> (Vec<Part>, Vec<(f32, f32, f32)>) {
    match cat {
        Cat::Sleep => (
            vec![
                part(BODY),
                part(TAIL_WAVE),
                part("M40 59 q5 4.5 10 0 M64 59 q5 4.5 10 0"),
                thin("M93 18 h9 l-9 9 h9", 3.0, 0.7, Role::Plain),
                thin("M107 7 h6 l-6 6 h6", 3.0, 0.7, Role::Plain),
            ],
            vec![],
        ),
        Cat::Hello => (vec![part(BODY), part(TAIL_CURL), part("M40 60 q5 -5.5 10 0")], vec![(69.0, 58.0, 3.8)]),
        Cat::Loading => (
            vec![
                part(BODY_TALL),
                thin(TAIL_WAVE, 4.5, 1.0, Role::Tail),
                thin("M95 18 q6 6 0 13", 3.5, 0.7, Role::Arc(0)),
                thin("M102 12 q11 12 0 25", 3.5, 0.7, Role::Arc(1)),
            ],
            vec![(45.0, 58.0, 3.8), (69.0, 58.0, 3.8)],
        ),
        Cat::Offline => (
            vec![
                part("M26 90 Q17 66 25 47 L14 32 L40 36 L74 36 L100 32 L89 47 Q97 66 88 90 Z"),
                part(TAIL_FLAT),
                thin("M39 51 L50 48 M75 51 L64 48", 3.5, 1.0, Role::Plain),
            ],
            vec![(45.0, 60.0, 3.6), (69.0, 60.0, 3.6)],
        ),
        Cat::NotFound => (
            vec![part(BODY), part(TAIL_WAVE), thin("M96 14 q0 -8 8 -8 q8 0 8 8 q0 6 -8 8 v5", 3.5, 1.0, Role::Plain)],
            vec![(49.0, 55.0, 3.8), (73.0, 55.0, 3.8), (104.0, 35.0, 2.4)],
        ),
        Cat::Peek => (
            vec![
                part("M24 70 Q21 58 25 47 L30 22 L42 34 L72 34 L84 22 L89 47 Q93 58 90 70"),
                part("M4 70 L120 70"),
                part("M36 70 q5 -7 10 0 M68 70 q5 -7 10 0"),
            ],
            vec![(45.0, 58.0, 3.8), (69.0, 58.0, 3.8)],
        ),
        Cat::Success => (
            vec![
                part(BODY),
                part(TAIL_CURL),
                part("M40 61 q5 -5.5 10 0 M64 61 q5 -5.5 10 0"),
                thin("M100 14 v10 M95 19 h10", 3.0, 1.0, Role::Plus(100.0, 19.0)),
                thin("M14 26 v8 M10 30 h8", 3.0, 1.0, Role::Plus(14.0, 30.0)),
                thin("M110 40 v6 M107 43 h6", 3.0, 1.0, Role::Plus(110.0, 43.0)),
            ],
            vec![],
        ),
        Cat::NoMic => (
            vec![
                part(BODY_TALL),
                part(TAIL_FLAT),
                // значок микрофона (translate 94 8): дуга подставки — двумя четвертями, как в исходном "a7.5 7.5 0 0 0 15 0"
                // корпус — <rect x=7 y=1.5 w=7 h=12 rx=3.5>: две полуокружности (по две четверти) и стенки
                thin("M101 13 q0 -3.5 3.5 -3.5 q3.5 0 3.5 3.5 v5 q0 3.5 -3.5 3.5 q-3.5 0 -3.5 -3.5 Z", 2.6, 1.0, Role::Plain),
                thin("M97 18.5 q0 7.5 7.5 7.5 q7.5 0 7.5 -7.5 M104.5 26 v4 M95 9 l20 20", 2.6, 1.0, Role::Plain),
            ],
            vec![(45.0, 58.0, 3.8), (69.0, 58.0, 3.8)],
        ),
    }
}

/// Подпуть: точки и признак «замкнут».
struct Sub {
    pts: Vec<(f32, f32)>,
    closed: bool,
}

/// Разбор пути SVG (подмножество: M L H V Q T Z, заглавные и строчные). Кривые — по 10 отрезков.
fn parse(d: &str) -> Vec<Sub> {
    // токены: команды и числа (знак минус начинает новое число)
    let mut toks: Vec<Result<char, f32>> = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, toks: &mut Vec<Result<char, f32>>| {
        if !num.is_empty() {
            toks.push(Err(num.parse().unwrap_or(0.0)));
            num.clear();
        }
    };
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            flush(&mut num, &mut toks);
            toks.push(Ok(ch));
        } else if ch == '-' {
            flush(&mut num, &mut toks);
            num.push(ch);
        } else if ch.is_ascii_digit() || ch == '.' {
            num.push(ch);
        } else {
            flush(&mut num, &mut toks);
        }
    }
    flush(&mut num, &mut toks);

    let mut subs: Vec<Sub> = Vec::new();
    let (mut cur, mut start, mut ctrl) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32), None::<(f32, f32)>);
    let mut cmd = 'M';
    let mut i = 0;
    let nums = |i: &mut usize, n: usize| -> Option<Vec<f32>> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            match toks.get(*i) {
                Some(Err(x)) => v.push(*x),
                _ => return None,
            }
            *i += 1;
        }
        Some(v)
    };
    let quad = |sub: &mut Sub, a: (f32, f32), c: (f32, f32), b: (f32, f32)| {
        for k in 1..=10 {
            let s = k as f32 / 10.0;
            let u = 1.0 - s;
            sub.pts.push((u * u * a.0 + 2.0 * u * s * c.0 + s * s * b.0, u * u * a.1 + 2.0 * u * s * c.1 + s * s * b.1));
        }
    };
    loop {
        if let Some(Ok(c)) = toks.get(i) {
            cmd = *c;
            i += 1;
            if cmd == 'Z' || cmd == 'z' {
                if let Some(s) = subs.last_mut() {
                    s.closed = true;
                }
                cur = start;
                ctrl = None;
                continue;
            }
        }
        if i >= toks.len() {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { (0.0, 0.0) };
        let next_ctrl;
        match cmd.to_ascii_uppercase() {
            'M' => {
                let Some(v) = nums(&mut i, 2) else { break };
                cur = (base.0 + v[0], base.1 + v[1]);
                start = cur;
                subs.push(Sub { pts: vec![cur], closed: false });
                cmd = if rel { 'l' } else { 'L' }; // следующие пары — как L
                next_ctrl = None;
            }
            'L' => {
                let Some(v) = nums(&mut i, 2) else { break };
                cur = (base.0 + v[0], base.1 + v[1]);
                next_ctrl = None;
            }
            'H' => {
                let Some(v) = nums(&mut i, 1) else { break };
                cur.0 = if rel { cur.0 + v[0] } else { v[0] };
                next_ctrl = None;
            }
            'V' => {
                let Some(v) = nums(&mut i, 1) else { break };
                cur.1 = if rel { cur.1 + v[0] } else { v[0] };
                next_ctrl = None;
            }
            'Q' => {
                let Some(v) = nums(&mut i, 4) else { break };
                let c = (base.0 + v[0], base.1 + v[1]);
                let b = (base.0 + v[2], base.1 + v[3]);
                if let Some(s) = subs.last_mut() {
                    quad(s, cur, c, b);
                }
                cur = b;
                next_ctrl = Some(c);
            }
            'T' => {
                let Some(v) = nums(&mut i, 2) else { break };
                let c = ctrl.map_or(cur, |c| (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1));
                let b = (base.0 + v[0], base.1 + v[1]);
                if let Some(s) = subs.last_mut() {
                    quad(s, cur, c, b);
                }
                cur = b;
                next_ctrl = Some(c);
            }
            _ => {
                i += 1; // неизвестная команда — пропустить
                continue;
            }
        }
        if !matches!(cmd.to_ascii_uppercase(), 'Q' | 'T')
            && let Some(s) = subs.last_mut()
            && s.pts.last() != Some(&cur)
        {
            s.pts.push(cur);
        }
        ctrl = next_ctrl;
    }
    subs
}

fn stroke_sub(painter: &Painter, pts: Vec<Pos2>, closed: bool, width: f32, color: Color32) {
    if pts.len() < 2 {
        return;
    }
    // скруглённые концы и стыки (как stroke-linecap/linejoin="round")
    if !closed {
        painter.circle_filled(pts[0], width / 2.0, color);
        painter.circle_filled(pts[pts.len() - 1], width / 2.0, color);
    }
    let shape = if closed { Shape::closed_line(pts, Stroke::new(width, color)) } else { Shape::line(pts, Stroke::new(width, color)) };
    painter.add(shape);
}

/// Нарисовать кота в `rect` (по ширине, пропорции 124×100). `time` — секунды анимации: для Loading — бегущее время,
/// для Success — сколько прошло с появления; None — статично.
pub fn cat(painter: &Painter, rect: Rect, which: Cat, color: Color32, time: Option<f64>) {
    let s = (rect.width() / 124.0).min(rect.height() / 100.0);
    let o = rect.center() - vec2(62.0, 50.0) * s;
    let map = |(x, y): (f32, f32)| o + vec2(x, y) * s;
    let (parts, dots) = shape(which);
    for part in &parts {
        let mut alpha = part.alpha;
        let mut scale_around: Option<((f32, f32), f32)> = None;
        match (part.role, time) {
            (Role::Tail, Some(t)) if which == Cat::Loading => {
                // хвост-волна бежит: синус периода 20 от основания (88, 90), у основания — плавно
                let phase = (t * 6.0) as f32;
                let pts: Vec<Pos2> = (0..=28)
                    .map(|k| {
                        let x = 88.0 + k as f32;
                        let env = (k as f32 / 6.0).min(1.0);
                        map((x, 90.0 - 4.0 * env * ((x - 88.0) * std::f32::consts::PI / 10.0 - phase).sin()))
                    })
                    .collect();
                stroke_sub(painter, pts, false, part.width * s, color);
                continue;
            }
            (Role::Arc(n), Some(t)) => {
                let k = 0.5 + 0.5 * ((t * 4.0) as f32 - n as f32 * 1.3).sin();
                alpha *= 0.3 + 0.7 * k;
            }
            (Role::Plus(cx, cy), Some(t)) => {
                // плюсики мягко появляются за 300 мс (по одному, с небольшим сдвигом)
                let k = (((t as f32) - 0.06 * (cx / 50.0)) / 0.3).clamp(0.0, 1.0);
                let e = 1.0 - (1.0 - k) * (1.0 - k);
                alpha *= e;
                scale_around = Some(((cx, cy), 0.4 + 0.6 * e));
            }
            _ => {}
        }
        if alpha <= 0.01 {
            continue;
        }
        let c = color.gamma_multiply(alpha);
        for sub in parse(part.d) {
            let pts = sub
                .pts
                .iter()
                .map(|&(x, y)| match scale_around {
                    Some(((cx, cy), k)) => map((cx + (x - cx) * k, cy + (y - cy) * k)),
                    None => map((x, y)),
                })
                .collect();
            stroke_sub(painter, pts, sub.closed, part.width * s, c);
        }
    }
    for (x, y, r) in dots {
        painter.circle_filled(map((x, y)), r * s, color);
    }
}

/// Время анимации кота для этого места экрана: Loading — бегущее (и перерисовка до 30 к/с), Success — с момента появления
/// (перерисовка только первые 300 мс). «Анимации: Выкл.», окно не на виду — None (статично, без перерисовок).
fn anim_time(ui: &Ui, which: Cat) -> Option<f64> {
    let ctx = ui.ctx();
    match which {
        Cat::Loading if crate::anim::important(ctx) => {
            ctx.request_repaint_after(crate::anim::SPEECH_FRAME);
            Some(ctx.input(|i| i.time))
        }
        Cat::Success => {
            if crate::anim::level() == crate::config::AnimPref::Off {
                return None;
            }
            // момент появления: не рисовался больше 0.5 с — появился заново
            let id = ui.id().with("cat-success");
            let now = ctx.input(|i| i.time);
            let (first, last) = ctx.data(|d| d.get_temp::<(f64, f64)>(id)).unwrap_or((now, now));
            let first = if now - last > 0.5 { now } else { first };
            ctx.data_mut(|d| d.insert_temp(id, (first, now)));
            let since = now - first;
            if since < 0.4 {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            Some(since)
        }
        _ => None,
    }
}

/// Кот (≈120 px), заголовок (16, жирный) и подпись (13), всё по центру; кнопка по желанию. true — нажата кнопка.
pub fn cat_state(ui: &mut Ui, which: Cat, title: &str, subtitle: &str, action: Option<&str>) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(120.0, 97.0), Sense::hover());
        let time = anim_time(ui, which);
        cat(ui.painter(), rect, which, t::p().accent, time);
        let p = t::p();
        ui.add_space(14.0);
        ui.label(RichText::new(title).font(t::bold(16.0)).color(p.text));
        ui.add_space(2.0);
        ui.label(RichText::new(subtitle).size(13.0).color(p.text_muted));
        if let Some(a) = action {
            ui.add_space(14.0);
            clicked = button(ui, ButtonKind::Primary, a, None).clicked();
        }
    });
    clicked
}

/// Кот без подписей (декоративный: экран входа), статичный.
pub fn cat_figure(painter: &Painter, rect: Rect, which: Cat) {
    cat(painter, rect, which, t::p().accent, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_body_closed_with_curves() {
        let subs = parse(BODY);
        assert_eq!(subs.len(), 1);
        assert!(subs[0].closed);
        assert_eq!(subs[0].pts[0], (26.0, 90.0));
        assert!(subs[0].pts.contains(&(30.0, 22.0)) && subs[0].pts.contains(&(84.0, 22.0)));
        // две кривые по 10 точек + M + 5 L
        assert_eq!(subs[0].pts.len(), 1 + 10 + 5 + 10);
    }

    #[test]
    fn parse_relative_and_smooth() {
        let subs = parse("M88 90 q5 -8 10 0 t10 0 t8 0");
        let last = *subs[0].pts.last().unwrap();
        assert!((last.0 - 116.0).abs() < 1e-4 && (last.1 - 90.0).abs() < 1e-4);
        // T отражает контрольную точку: второй горб — вниз
        let mid2 = subs[0].pts[15];
        assert!(mid2.1 > 90.0, "второй горб ниже линии: {mid2:?}");
    }

    #[test]
    fn parse_hv_and_multiple_subpaths() {
        let subs = parse("M93 18 h9 l-9 9 h9 M100 14 v10");
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].pts, vec![(93.0, 18.0), (102.0, 18.0), (93.0, 27.0), (102.0, 27.0)]);
        assert_eq!(subs[1].pts, vec![(100.0, 14.0), (100.0, 24.0)]);
    }

    #[test]
    fn every_cat_parses() {
        for c in [Cat::Sleep, Cat::Hello, Cat::Loading, Cat::Offline, Cat::NotFound, Cat::Peek, Cat::Success, Cat::NoMic] {
            let (parts, _) = shape(c);
            for p in parts {
                let subs = parse(p.d);
                assert!(!subs.is_empty() && subs.iter().all(|s| s.pts.len() >= 2), "{c:?}: {}", p.d);
                assert!(subs.iter().flat_map(|s| &s.pts).all(|&(x, y)| (0.0..=124.0).contains(&x) && (0.0..=100.0).contains(&y)), "{c:?} вне макета");
            }
        }
    }
}
