//! Общее состояние приложения, которое команды получают через `State<AppState>`.
//! По мере роста сюда добавятся дескриптор аудиодвижка и пул соединений к БД.

use std::sync::Arc;

use crate::audio::EngineHandle;
use crate::library::LibraryStore;
use crate::playback::Controls;
use crate::lyrics::LyricsScan;
use crate::replaygain::GainScan;
use crate::playlist::PlaylistStore;
use crate::lastfm::Scrobbler;
use crate::settings::SettingsStore;
use crate::spectrum::SpectrumBridge;

pub struct AppState {
    pub settings: Arc<SettingsStore>,
    pub playlist: Arc<PlaylistStore>,
    pub library: Arc<LibraryStore>,
    pub engine: EngineHandle,
    pub spectrum: Arc<SpectrumBridge>,
    pub scrobbler: Scrobbler,
    /// Фоновый подсчёт громкости (ReplayGain своими силами).
    pub gain: Arc<GainScan>,
    /// Пакетная загрузка текстов песен.
    pub lyrics: Arc<LyricsScan>,
    /// Пакетный поиск обложек.
    pub covers: Arc<crate::library::CoverScan>,
    /// Таймер сна и «остановить после этого трека».
    pub sleep: Arc<crate::sleep::SleepTimer>,
}

impl AppState {
    /// Общие действия воспроизведения — те же, что у медиаклавиш и трея.
    pub fn controls(&self) -> Controls<'_> {
        Controls {
            engine: &self.engine,
            playlist: &self.playlist,
            settings: &self.settings,
        }
    }
}
