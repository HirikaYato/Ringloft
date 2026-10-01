//! Кольцо между потоком движка и RT-колбэком: SPSC rtrb + атомики.
//!
//! Сброс кольца (стоп, смена трека, будущий seek) нельзя сделать со стороны
//! продюсера: в SPSC читать умеет только консьюмер, а консьюмер у нас — сам
//! аудиоколлбэк. Поэтому продюсер поднимает счётчик поколений, а колбэк,
//! увидев расхождение, выбрасывает всё содержимое одним `commit_all()` (O(1),
//! без аллокаций) и подтверждает поколение обратно.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Сколько ждём подтверждения сброса от колбэка, прежде чем ругаться в лог.
const FLUSH_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Debug)]
pub struct RingShared {
    /// Фреймов отдано устройству с последнего сброса.
    frames_played: AtomicU64,
    generation: AtomicU64,
    generation_ack: AtomicU64,
    /// Целевая громкость (биты f32), колбэк подтягивается к ней плавно.
    volume: AtomicU32,
    /// Множитель затухания (таймер сна) поверх громкости: громкость
    /// пользователя он не трогает. Сглаживается тем же покадровым шагом.
    fade: AtomicU32,
    /// false — колбэк выдаёт тишину, но поток устройства продолжает крутиться.
    playing: AtomicBool,
    /// Движок ещё собирается доливать сэмплы. Когда false, пустое кольцо —
    /// это нормальный конец трека, а не пропуск.
    expecting_data: AtomicBool,
    underruns: AtomicU64,
}

impl RingShared {
    fn new(volume: f32) -> Self {
        Self {
            frames_played: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            generation_ack: AtomicU64::new(0),
            volume: AtomicU32::new(volume.to_bits()),
            fade: AtomicU32::new(1.0f32.to_bits()),
            playing: AtomicBool::new(false),
            expecting_data: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn frames_played(&self) -> u64 {
        self.frames_played.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, value: f32) {
        self.volume.store(value.to_bits(), Ordering::Relaxed);
    }

    #[inline]
    pub fn fade(&self) -> f32 {
        f32::from_bits(self.fade.load(Ordering::Relaxed))
    }

    pub fn set_fade(&self, value: f32) {
        self.fade.store(value.to_bits(), Ordering::Relaxed);
    }

    #[inline]
    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn set_playing(&self, value: bool) {
        self.playing.store(value, Ordering::Relaxed);
    }

    #[inline]
    pub fn expecting_data(&self) -> bool {
        self.expecting_data.load(Ordering::Relaxed)
    }

    pub fn set_expecting_data(&self, value: bool) {
        self.expecting_data.store(value, Ordering::Relaxed);
    }

    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }
}

pub fn channel(capacity_samples: usize, volume: f32) -> (RingProducer, RingConsumer) {
    let (tx, rx) = rtrb::RingBuffer::<f32>::new(capacity_samples);
    let shared = Arc::new(RingShared::new(volume));
    (
        RingProducer {
            tx,
            shared: Arc::clone(&shared),
        },
        RingConsumer { rx, shared },
    )
}

pub struct RingProducer {
    tx: rtrb::Producer<f32>,
    shared: Arc<RingShared>,
}

impl RingProducer {
    pub fn shared(&self) -> &Arc<RingShared> {
        &self.shared
    }

    pub fn capacity(&self) -> usize {
        self.tx.buffer().capacity()
    }

    /// Свободных сэмплов (не фреймов).
    pub fn free(&self) -> usize {
        self.tx.slots()
    }

    /// Кольцо пусто — колбэк выбрал всё, что мы написали.
    pub fn is_drained(&self) -> bool {
        self.tx.slots() == self.capacity()
    }

    /// Пишет сколько влезает, возвращает число записанных сэмплов.
    pub fn write(&mut self, data: &[f32]) -> usize {
        let n = data.len().min(self.tx.slots());
        if n == 0 {
            return 0;
        }
        match self.tx.write_chunk_uninit(n) {
            Ok(chunk) => {
                let written = chunk.fill_from_iter(data[..n].iter().copied());
                debug_assert_eq!(written, n);
                written
            }
            Err(_) => 0,
        }
    }

    /// Объявляет содержимое кольца неактуальным и ждёт подтверждения колбэка.
    pub fn flush(&mut self) {
        let target = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;

        // Кольцо и так пусто: колбэк не трогает счётчик фреймов, можно
        // подтвердить поколение самим и не ждать (важно, когда потока ещё нет).
        if self.is_drained() {
            self.shared.frames_played.store(0, Ordering::Release);
            self.shared.generation_ack.store(target, Ordering::Release);
            return;
        }

        let deadline = Instant::now() + FLUSH_TIMEOUT;
        while self.shared.generation_ack.load(Ordering::Acquire) < target {
            if Instant::now() >= deadline {
                tracing::warn!("аудиоколлбэк не подтвердил сброс кольца за 200 мс");
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

pub struct RingConsumer {
    rx: rtrb::Consumer<f32>,
    shared: Arc<RingShared>,
}

impl RingConsumer {
    /// Вызывается первой строкой колбэка. Аллокаций и блокировок нет.
    #[inline]
    pub fn sync_generation(&mut self) {
        let generation = self.shared.generation.load(Ordering::Acquire);
        if generation == self.shared.generation_ack.load(Ordering::Relaxed) {
            return;
        }
        let pending = self.rx.slots();
        if pending > 0 && let Ok(chunk) = self.rx.read_chunk(pending) {
            chunk.commit_all();
        }
        self.shared.frames_played.store(0, Ordering::Release);
        self.shared.generation_ack.store(generation, Ordering::Release);
    }

    /// Забирает в `out` сколько есть, возвращает число сэмплов.
    #[inline]
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        let n = out.len().min(self.rx.slots());
        if n == 0 {
            return 0;
        }
        match self.rx.read_chunk(n) {
            Ok(chunk) => {
                let (first, second) = chunk.as_slices();
                out[..first.len()].copy_from_slice(first);
                out[first.len()..first.len() + second.len()].copy_from_slice(second);
                chunk.commit_all();
                n
            }
            Err(_) => 0,
        }
    }

    #[inline]
    pub fn advance_frames(&self, frames: usize) {
        self.shared
            .frames_played
            .fetch_add(frames as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn note_underrun(&self) {
        self.shared.underruns.fetch_add(1, Ordering::Relaxed);
    }

    pub fn shared(&self) -> &Arc<RingShared> {
        &self.shared
    }
}
