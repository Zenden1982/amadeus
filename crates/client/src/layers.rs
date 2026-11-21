//! Выбор слоя трансляции у зрителя (свой simulcast): «исходное» или «экономное» (720p30).
//! Авто: большие потери — сразу вниз, умеренные потери или заморозки — два замера подряд; долго чисто — пробуем вверх. Если наверху снова плохо вскоре после подъёма,
//! следующая попытка — позже (30 с → 60 → 120 → … до 5 мин), чтобы не прыгать туда-сюда.

use std::time::{Duration, Instant};

/// Что выбрал человек.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Quality {
    #[default]
    Auto,
    High,
    Low,
}

impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Quality::Auto => "Авто",
            Quality::High => "Исходное",
            Quality::Low => "Экономное",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    High,
    Low,
}

/// Замер приёма за интервал статистики.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    /// Доля потерянных пакетов видео (0..1).
    pub loss: f64,
    /// Новых заморозок картинки.
    pub freezes: u32,
}

impl Sample {
    fn bad(&self) -> bool {
        self.loss >= LOSS_BAD || self.freezes > 0
    }
    /// Сразу вниз — только при больших потерях; заморозка (вместе с потерями) — как обычное «плохо», нужно два замера.
    fn very_bad(&self) -> bool {
        self.loss >= LOSS_VERY_BAD
    }
}

const LOSS_BAD: f64 = 0.04;
const LOSS_VERY_BAD: f64 = 0.10;
const BACKOFF_START: Duration = Duration::from_secs(30);
const BACKOFF_MAX: Duration = Duration::from_secs(300);
/// Подъём «не удался», если плохо стало быстрее этого.
const FAILED_UP: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct AutoLayer {
    layer: Layer,
    bad_streak: u32,
    good_since: Option<Instant>,
    last_switch: Option<(Instant, Layer)>,
    backoff: Duration,
}

impl Default for AutoLayer {
    fn default() -> Self {
        Self { layer: Layer::High, bad_streak: 0, good_since: None, last_switch: None, backoff: BACKOFF_START }
    }
}

impl AutoLayer {
    pub fn layer(&self) -> Layer {
        self.layer
    }

    /// Новый замер; Some — пора сменить слой.
    pub fn update(&mut self, now: Instant, s: Sample) -> Option<Layer> {
        match self.layer {
            Layer::High => {
                if !s.bad() {
                    self.bad_streak = 0;
                    return None;
                }
                self.bad_streak += 1;
                if !(s.very_bad() || self.bad_streak >= 2) {
                    return None;
                }
                // вскоре после подъёма снова плохо — следующий подъём позже
                if let Some((t, Layer::High)) = self.last_switch
                    && now.duration_since(t) < FAILED_UP
                {
                    self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
                }
                self.switch(now, Layer::Low)
            }
            Layer::Low => {
                if s.bad() {
                    self.good_since = None;
                    return None;
                }
                let since = *self.good_since.get_or_insert(now);
                if now.duration_since(since) >= self.backoff {
                    self.switch(now, Layer::High)
                } else {
                    None
                }
            }
        }
    }

    fn switch(&mut self, now: Instant, to: Layer) -> Option<Layer> {
        self.layer = to;
        self.bad_streak = 0;
        self.good_since = None;
        self.last_switch = Some((now, to));
        Some(to)
    }
}

/// Доля потерь и новые заморозки между двумя замерами счётчиков (потеряно, принято, заморозок).
/// Заморозка без потерь не считается: захват окна шлёт кадры только при изменениях, и на статичной картинке
/// WebRTC видит «заморозку», хотя сеть ни при чём.
pub fn sample(prev: (i64, u64, u32), cur: (i64, u64, u32)) -> Sample {
    let lost = (cur.0 - prev.0).max(0) as f64;
    let got = cur.1.saturating_sub(prev.1) as f64;
    let loss = if lost + got > 0.0 { lost / (lost + got) } else { 0.0 };
    let freezes = if lost > 0.0 { cur.2.saturating_sub(prev.2) } else { 0 };
    Sample { loss, freezes }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: Sample = Sample { loss: 0.0, freezes: 0 };
    const BAD: Sample = Sample { loss: 0.05, freezes: 0 };
    const AWFUL: Sample = Sample { loss: 0.2, freezes: 0 };
    const FREEZE: Sample = Sample { loss: 0.0, freezes: 1 };

    fn at(t0: Instant, s: u64) -> Instant {
        t0 + Duration::from_secs(s)
    }

    #[test]
    fn down_fast_up_slow() {
        let t0 = Instant::now();
        let mut a = AutoLayer::default();
        assert_eq!(a.update(at(t0, 2), BAD), None, "одна небольшая потеря — ещё терпим");
        assert_eq!(a.update(at(t0, 4), BAD), Some(Layer::Low), "две подряд — вниз");
        // 28 с чисто — рано
        for s in (6..=32).step_by(2) {
            assert_eq!(a.update(at(t0, s), GOOD), None, "{s}");
        }
        assert_eq!(a.update(at(t0, 36), GOOD), Some(Layer::High), "30 с чисто — вверх");
    }

    #[test]
    fn heavy_loss_drops_at_once_freeze_needs_two() {
        let t0 = Instant::now();
        let mut a = AutoLayer::default();
        assert_eq!(a.update(at(t0, 2), FREEZE), None, "одна заморозка — ещё терпим");
        assert_eq!(a.update(at(t0, 4), FREEZE), Some(Layer::Low));
        let mut b = AutoLayer::default();
        assert_eq!(b.update(at(t0, 2), AWFUL), Some(Layer::Low));
    }

    #[test]
    fn failed_climb_doubles_wait() {
        let t0 = Instant::now();
        let mut a = AutoLayer::default();
        a.update(at(t0, 0), AWFUL); // вниз
        a.update(at(t0, 2), GOOD);
        assert_eq!(a.update(at(t0, 32), GOOD), Some(Layer::High));
        assert_eq!(a.update(at(t0, 40), AWFUL), Some(Layer::Low), "наверху сразу плохо");
        a.update(at(t0, 42), GOOD);
        assert_eq!(a.update(at(t0, 72), GOOD), None, "теперь ждём 60 с, а не 30");
        assert_eq!(a.update(at(t0, 102), GOOD), Some(Layer::High));
    }

    #[test]
    fn bad_sample_resets_good_streak() {
        let t0 = Instant::now();
        let mut a = AutoLayer::default();
        a.update(at(t0, 0), AWFUL);
        a.update(at(t0, 2), GOOD);
        a.update(at(t0, 20), BAD);
        a.update(at(t0, 22), GOOD);
        assert_eq!(a.update(at(t0, 40), GOOD), None, "отсчёт начался заново на 22 с");
        assert_eq!(a.update(at(t0, 52), GOOD), Some(Layer::High));
    }

    #[test]
    fn sample_math() {
        let s = sample((10, 1000, 2), (20, 1090, 3));
        assert!((s.loss - 0.1).abs() < 1e-9);
        assert_eq!(s.freezes, 1);
        assert_eq!(sample((5, 10, 0), (5, 10, 0)).loss, 0.0);
        assert_eq!(sample((5, 10, 0), (5, 90, 2)).freezes, 0, "заморозка без потерь — статичная картинка");
    }
}
