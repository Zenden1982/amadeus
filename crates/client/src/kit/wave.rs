//! Волна — общий элемент: речь (рядом с ником, на плитке) и пинг в заголовке канала. Рисуется ломаной, не картинкой.

use eframe::egui::{Color32, Painter, Pos2, Shape, Stroke, pos2};

/// Период узора, px.
const PERIOD: f32 = 20.0;
/// Точки узора на периоде: (x, y), y — от центра при амплитуде 4.5.
const PATTERN: [(f32, f32); 8] = [(0.0, 0.0), (3.0, 0.0), (5.0, -4.5), (8.0, 4.5), (10.0, -2.5), (12.0, 2.5), (14.0, 0.0), (20.0, 0.0)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaveStyle {
    Solid,
    /// Пунктир (потери пакетов).
    Dashed,
}

/// y узора в точке `u` (0..PERIOD), при амплитуде 4.5.
fn pattern_y(u: f32) -> f32 {
    for w in PATTERN.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if u <= x1 {
            return y0 + (y1 - y0) * ((u - x0) / (x1 - x0)).clamp(0.0, 1.0);
        }
    }
    0.0
}

/// Точки волны от `left` (центр по высоте) шириной `width`; `amp` — размах вверх/вниз в px, `phase` — сдвиг узора в px
/// (растёт — линия «бежит» вправо).
pub fn wave_points(left: Pos2, width: f32, amp: f32, phase: f32) -> Vec<Pos2> {
    let k = amp / 4.5;
    let shift = phase.rem_euclid(PERIOD);
    let mut xs: Vec<f32> = vec![0.0, width];
    let mut n = -1.0;
    while n * PERIOD + shift < width + PERIOD {
        for (px, _) in PATTERN {
            let x = n * PERIOD + shift + px;
            if x > 0.0 && x < width {
                xs.push(x);
            }
        }
        n += 1.0;
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    xs.into_iter().map(|x| pos2(left.x + x, left.y + pattern_y((x - shift).rem_euclid(PERIOD)) * k)).collect()
}

/// Нарисовать волну: толщина 2 px, скруглённые концы.
pub fn wave(painter: &Painter, left: Pos2, width: f32, amp: f32, phase: f32, color: Color32, style: WaveStyle) {
    let pts = wave_points(left, width, amp, phase);
    let stroke = Stroke::new(2.0, color);
    match style {
        WaveStyle::Solid => {
            for q in [pts[0], pts[pts.len() - 1]] {
                painter.circle_filled(q, 1.0, color);
            }
            painter.add(Shape::line(pts, stroke));
        }
        WaveStyle::Dashed => {
            painter.extend(Shape::dashed_line(&pts, stroke, 4.0, 3.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_spans_width_and_amplitude() {
        let pts = wave_points(pos2(10.0, 50.0), 64.0, 9.0, 7.0);
        assert_eq!(pts.first().unwrap().x, 10.0);
        assert_eq!(pts.last().unwrap().x, 74.0);
        assert!(pts.windows(2).all(|w| w[0].x <= w[1].x), "по возрастанию x");
        let max = pts.iter().map(|p| (p.y - 50.0).abs()).fold(0.0, f32::max);
        assert!((max - 9.0).abs() < 0.01, "размах = amp, а не {max}");
    }

    #[test]
    fn flat_when_no_amplitude() {
        assert!(wave_points(pos2(0.0, 0.0), 28.0, 0.0, 3.0).iter().all(|p| p.y == 0.0));
    }
}
