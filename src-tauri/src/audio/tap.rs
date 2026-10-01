//! Отвод звука для анализатора спектра.
//!
//! Колбэк отдаёт сюда то, что реально уходит в устройство, и никогда не ждёт:
//! если анализатор отстал, кадр просто теряется. Картинка от этого дрогнет,
//! а звук — нет, и это единственно верный размен.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct TapProducer {
    tx: rtrb::Producer<f32>,
    /// Пока никто не смотрит на спектр, колбэк не тратит на него ни такта.
    active: Arc<AtomicBool>,
}

pub struct TapConsumer {
    rx: rtrb::Consumer<f32>,
    pub channels: usize,
    pub sample_rate: u32,
}

pub fn channel(
    capacity: usize,
    channels: usize,
    sample_rate: u32,
    active: Arc<AtomicBool>,
) -> (TapProducer, TapConsumer) {
    let (tx, rx) = rtrb::RingBuffer::<f32>::new(capacity);
    (
        TapProducer { tx, active },
        TapConsumer {
            rx,
            channels,
            sample_rate,
        },
    )
}

impl TapProducer {
    /// Вызывается из аудиоколлбэка: без аллокаций, без блокировок, без ожидания.
    #[inline]
    pub fn push(&mut self, samples: &[f32]) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }
        let count = samples.len().min(self.tx.slots());
        if count == 0 {
            return;
        }
        if let Ok(chunk) = self.tx.write_chunk_uninit(count) {
            chunk.fill_from_iter(samples[..count].iter().copied());
        }
    }
}

impl TapConsumer {
    /// Забирает всё накопившееся. Анализатору нужен самый свежий звук,
    /// поэтому старое он отбросит сам.
    pub fn drain(&mut self, out: &mut Vec<f32>) {
        let available = self.rx.slots();
        if available == 0 {
            return;
        }
        if let Ok(chunk) = self.rx.read_chunk(available) {
            let (first, second) = chunk.as_slices();
            out.extend_from_slice(first);
            out.extend_from_slice(second);
            chunk.commit_all();
        }
    }
}
