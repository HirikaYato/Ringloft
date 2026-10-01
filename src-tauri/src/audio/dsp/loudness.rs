//! Измерение громкости по ITU-R BS.1770 (оно же EBU R128) — для своего
//! ReplayGain там, где метки в файле нет.
//!
//! Порядок ровно как в стандарте: K-взвешивание двумя биквадами, затем
//! блоки по 400 мс с перекрытием 75 %, затем двойное гейтирование —
//! абсолютный порог −70 LUFS и относительный в −10 LU от среднего по
//! оставшимся блокам. Без гейтирования тишина между треками утягивала бы
//! измерение вниз, и тихие записи получали бы завышенное усиление.

/// Опорная громкость ReplayGain 2.0.
pub const REFERENCE_LUFS: f64 = -18.0;

/// Длина блока измерения и шаг между блоками.
const BLOCK_MS: u64 = 400;
const STEP_DIVISOR: u64 = 4;
/// Абсолютный порог гейта.
const ABSOLUTE_GATE: f64 = -70.0;
/// Насколько ниже среднего проходит относительный гейт.
const RELATIVE_GATE: f64 = -10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    /// Интегральная громкость, LUFS. `None` — измерять было нечего.
    pub lufs: Option<f64>,
    /// Пик в линейных единицах: 1.0 — полная шкала.
    pub peak: f32,
}

impl Loudness {
    /// Усиление ReplayGain: сколько добавить, чтобы выйти на опорный уровень.
    pub fn gain_db(&self) -> Option<f64> {
        self.lufs.map(|lufs| REFERENCE_LUFS - lufs)
    }
}

/// Один биквад прямой формы II транспонированной.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn process(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

/// Коэффициенты K-взвешивания. В стандарте они даны для 48 кГц, поэтому для
/// остальных частот пересчитываются: иначе на 44,1 кГц фильтр встаёт не туда.
fn pre_filter(rate: f64) -> Biquad {
    let f0 = 1681.974450955533;
    let gain_db = 3.999843853973347;
    let q = 0.7071752369554196;

    let k = (std::f64::consts::PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;

    Biquad {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

fn rlb_filter(rate: f64) -> Biquad {
    let f0 = 38.13547087602444;
    let q = 0.5003270373238773;
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let denominator = 1.0 + k / q + k * k;

    Biquad {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k * k - 1.0) / denominator,
        a2: (1.0 - k / q + k * k) / denominator,
        z1: 0.0,
        z2: 0.0,
    }
}

/// Накопитель: кормится блоками interleaved-сэмплов, на выходе даёт
/// интегральную громкость и пик.
pub struct Meter {
    channels: usize,
    /// Пары фильтров на каждый канал: состояние у них своё.
    filters: Vec<(Biquad, Biquad)>,
    /// Сумма квадратов внутри текущего шага, по каналам.
    sums: Vec<f64>,
    /// Готовые шаги: блок складывается из четырёх, отсюда и перекрытие 75 %.
    steps: Vec<Vec<f64>>,
    step_frames: usize,
    frames_in_step: usize,
    /// Средние квадраты по блокам — из них считается гейтирование.
    blocks: Vec<f64>,
    peak: f32,
}

impl Meter {
    pub fn new(rate: u32, channels: usize) -> Self {
        let channels = channels.max(1);
        let step_frames =
            (u64::from(rate) * BLOCK_MS / 1000 / STEP_DIVISOR).max(1) as usize;

        Self {
            channels,
            filters: (0..channels)
                .map(|_| (pre_filter(f64::from(rate)), rlb_filter(f64::from(rate))))
                .collect(),
            sums: vec![0.0; channels],
            steps: Vec::new(),
            step_frames,
            frames_in_step: 0,
            blocks: Vec::new(),
            peak: 0.0,
        }
    }

    /// Порция interleaved-сэмплов в формате источника.
    pub fn push(&mut self, samples: &[f32]) {
        for frame in samples.chunks_exact(self.channels) {
            for (channel, sample) in frame.iter().enumerate() {
                self.peak = self.peak.max(sample.abs());
                let filtered = {
                    let (pre, rlb) = &mut self.filters[channel];
                    rlb.process(pre.process(f64::from(*sample)))
                };
                self.sums[channel] += filtered * filtered;
            }

            self.frames_in_step += 1;
            if self.frames_in_step >= self.step_frames {
                self.close_step();
            }
        }
    }

    fn close_step(&mut self) {
        let frames = self.frames_in_step.max(1) as f64;
        let step: Vec<f64> = self.sums.iter().map(|sum| sum / frames).collect();
        self.steps.push(step);
        self.sums.iter_mut().for_each(|sum| *sum = 0.0);
        self.frames_in_step = 0;

        // Блок — это четыре шага подряд; дальше окно едет на один шаг.
        if self.steps.len() >= STEP_DIVISOR as usize {
            let start = self.steps.len() - STEP_DIVISOR as usize;
            let mut weighted = 0.0;
            for channel in 0..self.channels {
                let mean: f64 = self.steps[start..]
                    .iter()
                    .map(|step| step[channel])
                    .sum::<f64>()
                    / STEP_DIVISOR as f64;
                weighted += channel_weight(channel, self.channels) * mean;
            }
            self.blocks.push(weighted);
            // Больше четырёх шагов держать незачем.
            if self.steps.len() > STEP_DIVISOR as usize {
                self.steps.drain(..self.steps.len() - STEP_DIVISOR as usize);
            }
        }
    }

    /// Блоки измерения: из них считается альбомное усиление.
    pub fn blocks(&self) -> &[f64] {
        &self.blocks
    }

    pub fn finish(self) -> Loudness {
        Loudness {
            lufs: integrate(&self.blocks),
            peak: self.peak,
        }
    }
}

/// Громкость по готовым блокам: так считается альбом, у которого блоки
/// собраны со всех треков.
pub fn album_lufs(blocks: &[f64]) -> Option<f64> {
    integrate(blocks)
}

/// Вес канала: тыловые в стандарте громче на 1,5 дБ. Для стерео все единицы.
fn channel_weight(channel: usize, channels: usize) -> f64 {
    if channels >= 5 && channel >= 3 {
        1.4125375446227544
    } else {
        1.0
    }
}

/// Двойное гейтирование из стандарта.
fn integrate(blocks: &[f64]) -> Option<f64> {
    if blocks.is_empty() {
        return None;
    }

    let loud = |mean: f64| -0.691 + 10.0 * mean.max(f64::MIN_POSITIVE).log10();

    let above_absolute: Vec<f64> = blocks
        .iter()
        .copied()
        .filter(|mean| loud(*mean) > ABSOLUTE_GATE)
        .collect();
    if above_absolute.is_empty() {
        return None;
    }

    let average = above_absolute.iter().sum::<f64>() / above_absolute.len() as f64;
    let threshold = loud(average) + RELATIVE_GATE;

    let above_relative: Vec<f64> = above_absolute
        .into_iter()
        .filter(|mean| loud(*mean) > threshold)
        .collect();
    if above_relative.is_empty() {
        return None;
    }

    let mean = above_relative.iter().sum::<f64>() / above_relative.len() as f64;
    Some(loud(mean))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Стерео-синус 1 кГц заданной амплитуды.
    fn sine(amplitude: f32, seconds: f64, rate: u32) -> Vec<f32> {
        let frames = (seconds * f64::from(rate)) as usize;
        let mut samples = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let phase =
                2.0 * std::f64::consts::PI * 1000.0 * frame as f64 / f64::from(rate);
            let value = amplitude * phase.sin() as f32;
            samples.push(value);
            samples.push(value);
        }
        samples
    }

    fn measure(samples: &[f32], rate: u32) -> Loudness {
        let mut meter = Meter::new(rate, 2);
        // Кормим порциями, как это делает декодер.
        for chunk in samples.chunks(4096) {
            meter.push(chunk);
        }
        meter.finish()
    }

    /// У синуса 1 кГц громкость в LUFS совпадает с пиком в дБFS: усиление
    /// K-взвешивания на этой частоте ровно гасит смещение −0,691.
    /// Сверено с ffmpeg ebur128 (та же связь I == TPK).
    #[test]
    fn sine_matches_its_peak_level() {
        for amplitude in [1.0f32, 0.1, 0.01] {
            let expected = 20.0 * f64::from(amplitude).log10();
            let result = measure(&sine(amplitude, 6.0, 48_000), 48_000);
            let lufs = result.lufs.unwrap_or_default();
            assert!(
                (lufs - expected).abs() < 0.3,
                "амплитуда {amplitude}: {lufs} вместо {expected}"
            );
            assert!((result.peak - amplitude).abs() < 1e-3);
        }
    }

    /// Частота дискретизации не должна сдвигать результат: коэффициенты
    /// пересчитываются под неё.
    #[test]
    fn rate_does_not_shift_result() {
        let at_48 = measure(&sine(0.5, 6.0, 48_000), 48_000).lufs;
        let at_44 = measure(&sine(0.5, 6.0, 44_100), 44_100).lufs;
        let delta = at_48.unwrap_or_default() - at_44.unwrap_or_default();
        assert!(delta.abs() < 0.1, "{at_48:?} против {at_44:?}");
    }

    /// Тишина в конце трека не должна утягивать измерение вниз — за это
    /// отвечает гейтирование.
    #[test]
    fn silence_is_gated_out() {
        let mut with_tail = sine(0.5, 6.0, 48_000);
        let loud = measure(&with_tail, 48_000).lufs.unwrap_or_default();
        with_tail.extend(std::iter::repeat_n(0.0, 48_000 * 2 * 10));
        let with_silence = measure(&with_tail, 48_000).lufs.unwrap_or_default();
        assert!((loud - with_silence).abs() < 0.2, "{loud} против {with_silence}");
    }

    #[test]
    fn full_silence_has_no_loudness() {
        let result = measure(&vec![0.0; 48_000 * 2 * 3], 48_000);
        assert_eq!(result.lufs, None);
        assert_eq!(result.gain_db(), None);
        assert_eq!(result.peak, 0.0);
    }

    /// Слишком короткий фрагмент не даёт ни одного блока в 400 мс.
    #[test]
    fn too_short_has_no_loudness() {
        assert_eq!(measure(&sine(0.5, 0.2, 48_000), 48_000).lufs, None);
    }

    #[test]
    fn gain_aims_at_reference() {
        let result = measure(&sine(0.5, 6.0, 48_000), 48_000);
        let gain = result.gain_db().unwrap_or_default();
        let lufs = result.lufs.unwrap_or_default();
        assert!((lufs + gain - REFERENCE_LUFS).abs() < 1e-9);
    }
}
