//! Спектр для визуализатора: поток анализа и канал в вебвью.
//!
//! Считается в своём потоке и уходит через `tauri::ipc::Channel` — событиями
//! Tauri тридцать кадров в секунду слать накладно. Пока визуализатор закрыт,
//! поток спит, а колбэк вообще не трогает отвод.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use tauri::ipc::Channel;

use crate::audio::{Analyzer, EngineHandle, TapConsumer};

/// ~30 кадров в секунду: глазу достаточно, вебвью не захлёбывается.
const FRAME_PERIOD: Duration = Duration::from_millis(33);
/// Сколько полос рисуем.
const BANDS: usize = 56;
/// Как часто проверяем, не появился ли подписчик.
const IDLE_PERIOD: Duration = Duration::from_millis(150);

pub struct SpectrumBridge {
    channel: Mutex<Option<Channel<Vec<f32>>>>,
    engine: EngineHandle,
    active: AtomicBool,
}

impl SpectrumBridge {
    pub fn subscribe(&self, channel: Channel<Vec<f32>>) {
        *self.channel.lock() = Some(channel);
        self.active.store(true, Ordering::Relaxed);
        self.engine.set_analysis(true);
        tracing::debug!("визуализатор подписался на спектр");
    }

    pub fn unsubscribe(&self) {
        *self.channel.lock() = None;
        self.active.store(false, Ordering::Relaxed);
        self.engine.set_analysis(false);
        tracing::debug!("визуализатор отписался");
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// `false` — канал закрылся (окно ушло), подписку пора снять.
    fn send(&self, levels: &[f32]) -> bool {
        let guard = self.channel.lock();
        let Some(channel) = guard.as_ref() else {
            return false;
        };
        channel.send(levels.to_vec()).is_ok()
    }
}

pub fn spawn(engine: EngineHandle, taps: Receiver<TapConsumer>) -> Arc<SpectrumBridge> {
    let bridge = Arc::new(SpectrumBridge {
        channel: Mutex::new(None),
        engine,
        active: AtomicBool::new(false),
    });

    let weak = Arc::downgrade(&bridge);
    let spawned = std::thread::Builder::new()
        .name("ringloft-spectrum".into())
        .spawn(move || run(weak, taps));

    if let Err(err) = spawned {
        tracing::error!(%err, "не удалось запустить поток спектра");
    }
    bridge
}

fn run(bridge: Weak<SpectrumBridge>, taps: Receiver<TapConsumer>) {
    let mut consumer: Option<TapConsumer> = None;
    let mut analyzer: Option<Analyzer> = None;
    let mut buffer: Vec<f32> = Vec::new();
    let mut was_idle = true;

    loop {
        let Some(bridge) = bridge.upgrade() else { break };

        // Устройство могли переоткрыть — берём самый свежий отвод.
        while let Ok(fresh) = taps.try_recv() {
            analyzer = Some(Analyzer::new(fresh.sample_rate, BANDS));
            consumer = Some(fresh);
        }

        if !bridge.is_active() {
            if !was_idle {
                was_idle = true;
                if let Some(analyzer) = analyzer.as_mut() {
                    analyzer.reset();
                }
            }
            drop(bridge);
            std::thread::sleep(IDLE_PERIOD);
            continue;
        }
        was_idle = false;

        if let (Some(consumer), Some(analyzer)) = (consumer.as_mut(), analyzer.as_mut()) {
            buffer.clear();
            consumer.drain(&mut buffer);
            if !buffer.is_empty() {
                analyzer.feed(&buffer, consumer.channels);
            }
            if !bridge.send(analyzer.compute()) {
                bridge.unsubscribe();
            }
        }

        drop(bridge);
        std::thread::sleep(FRAME_PERIOD);
    }
    tracing::debug!("поток спектра остановлен");
}
