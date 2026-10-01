//! Настройки приложения: типы, значения по умолчанию, нормализация.
//!
//! Живут в `app_config_dir()/settings.json` и читаются бэкендом ДО создания
//! окна (устройство вывода, EQ, громкость нужны движку раньше, чем вебвью
//! вообще существует) — поэтому это не tauri-plugin-store, а свой стор.

mod headphone;
mod store;

pub use headphone::{FilterShape, HeadphoneProfile, PeqFilter, parse_parametric};
pub use store::SettingsStore;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Версия схемы: пригодится, когда понадобится миграция старого конфига.
pub const SCHEMA_VERSION: u32 = 1;

pub const EQ_BAND_COUNT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "Theme.ts")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "RepeatMode.ts")]
pub enum RepeatMode {
    #[default]
    Off,
    Track,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "ReplayGainMode.ts")]
pub enum ReplayGainMode {
    #[default]
    Off,
    Track,
    Album,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "PlaybackSettings.ts")]
pub struct PlaybackSettings {
    pub volume: f32,
    pub muted: bool,
    pub repeat: RepeatMode,
    pub shuffle: bool,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            volume: 0.7,
            muted: false,
            repeat: RepeatMode::default(),
            shuffle: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "AudioSettings.ts")]
pub struct AudioSettings {
    /// `None` — системное устройство по умолчанию.
    pub device_id: Option<String>,
    /// Целевое наполнение кольцевого буфера. Больше — устойчивее к пропускам,
    /// но дольше отклик EQ (он считается до буфера).
    pub buffer_ms: u32,
    pub replay_gain: ReplayGainMode,
    pub replay_gain_preamp_db: f32,
    /// 0 — режим gapless, иначе длительность кроссфейда.
    pub crossfade_ms: u32,
    pub eq_enabled: bool,
    pub eq_preamp_db: f32,
    pub eq_bands_db: Vec<f32>,
    /// Профиль наушников (AutoEQ) и включён ли он. Профиль хранится и когда
    /// выключен — чтобы включить обратно без сети.
    pub headphone: Option<HeadphoneProfile>,
    pub headphone_enabled: bool,
    /// Открывать устройство на частоте файла, если оно её поддерживает:
    /// звук идёт без пересчёта. Цена — пауза на стыке треков разной частоты.
    pub exact_rate: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            device_id: None,
            buffer_ms: 1500,
            replay_gain: ReplayGainMode::default(),
            replay_gain_preamp_db: 0.0,
            crossfade_ms: 0,
            eq_enabled: false,
            eq_preamp_db: 0.0,
            eq_bands_db: vec![0.0; EQ_BAND_COUNT],
            headphone: None,
            headphone_enabled: false,
            exact_rate: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "LibrarySettings.ts")]
pub struct LibrarySettings {
    pub folders: Vec<PathBuf>,
    pub watch: bool,
    pub follow_symlinks: bool,
    /// При запуске сверять плейлисты с их папками-источниками.
    pub refresh_playlists: bool,
}

// Не `derive(Default)`: обновление плейлистов при запуске включено.
impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            watch: false,
            follow_symlinks: false,
            refresh_playlists: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "MainView.ts")]
pub enum MainView {
    #[default]
    Playlist,
    Library,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "UiSettings.ts")]
pub struct UiSettings {
    pub theme: Theme,
    pub minimize_to_tray: bool,
    /// Раздел, открытый при прошлом закрытии окна.
    pub view: MainView,
    /// Показывать системное уведомление при смене трека.
    pub track_notifications: bool,
    /// Компактный режим: одна полоса плеера вместо всего окна.
    pub mini: bool,
    /// Держать окно поверх остальных.
    pub always_on_top: bool,
    /// Цвет акцента, `#rrggbb`. `None` — из темы.
    pub accent: Option<String>,
    /// Акцент берётся из обложки играющего трека; `accent` — запасной, для
    /// треков без обложки или с бесцветной.
    pub accent_from_cover: bool,
    /// Основной размер шрифта в пикселях; остальные размеры и высота строк
    /// списков считаются от него.
    pub font_size: u8,
    /// Шрифт интерфейса: встроенный по ключу (`system`, `inter`…) или имя
    /// установленного в системе семейства.
    pub font: String,
    /// Шрифт текста песни — так же.
    pub lyrics_font: String,
    /// Колонки плейлиста по порядку; ширина `None` — по умолчанию.
    pub playlist_columns: Vec<ColumnSetting>,
    /// Плейлисты вкладками сверху или списком слева.
    pub tab_layout: TabLayout,
    /// Ширина колонки плейлистов и свёрнута ли она в узкую полосу.
    pub sidebar_width: u32,
    pub sidebar_collapsed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TabLayout.ts")]
pub enum TabLayout {
    #[default]
    Tabs,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ColumnId.ts")]
pub enum ColumnId {
    Num,
    Title,
    Artist,
    Album,
    Year,
    Genre,
    Duration,
    Format,
    Bitrate,
    Plays,
    Rating,
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ColumnSetting.ts")]
pub struct ColumnSetting {
    pub id: ColumnId,
    pub width: Option<u32>,
}

fn default_columns() -> Vec<ColumnSetting> {
    [ColumnId::Num, ColumnId::Title, ColumnId::Artist, ColumnId::Album, ColumnId::Duration]
        .into_iter()
        .map(|id| ColumnSetting { id, width: None })
        .collect()
}

const FONT_SIZE_MIN: u8 = 12;
const FONT_SIZE_MAX: u8 = 20;
const FONT_SIZE_DEFAULT: u8 = 15;
const FONT_DEFAULT: &str = "system";
const LYRICS_FONT_DEFAULT: &str = "manrope";

// Не `derive(Default)`: уведомления о треке включены по умолчанию, а `bool`
// по умолчанию — `false`.
impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            minimize_to_tray: false,
            view: MainView::default(),
            track_notifications: true,
            mini: false,
            always_on_top: false,
            accent: None,
            accent_from_cover: false,
            font_size: FONT_SIZE_DEFAULT,
            font: FONT_DEFAULT.to_owned(),
            lyrics_font: LYRICS_FONT_DEFAULT.to_owned(),
            playlist_columns: default_columns(),
            tab_layout: TabLayout::default(),
            sidebar_width: 220,
            sidebar_collapsed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "Settings.ts")]
pub struct Settings {
    pub version: u32,
    pub playback: PlaybackSettings,
    pub audio: AudioSettings,
    pub library: LibrarySettings,
    pub ui: UiSettings,
    pub lastfm: LastfmSettings,
    pub discord: DiscordSettings,
    pub acoustid: AcoustidSettings,
    /// Умные списки, собранные пользователем. Встроенные живут во фронте:
    /// это те же правила, просто неизменяемые.
    pub smart_lists: Vec<SmartList>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SmartList.ts")]
pub struct SmartList {
    pub id: u64,
    pub name: String,
    pub rules: crate::library::SmartRules,
}

/// Идентификатор приложения Discord общий и живёт в `discord/`: здесь только
/// выключатель. Старое поле `clientId` в файле настроек просто игнорируется.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "DiscordSettings.ts")]
pub struct DiscordSettings {
    pub enabled: bool,
}

/// Ключ приложения AcoustID (теги по звуку) заводит пользователь на
/// acoustid.org/new-application — как и ключ Last.fm: чужой ключ — чужая квота.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "AcoustidSettings.ts")]
pub struct AcoustidSettings {
    pub api_key: Option<String>,
}

/// Ключ и секрет приложения Last.fm заводит сам пользователь: чужой ключ в
/// плеере — это чужая квота и чужая ответственность.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "LastfmSettings.ts")]
pub struct LastfmSettings {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    /// Ключ сессии: постоянный, отзывается на сайте Last.fm.
    pub session_key: Option<String>,
    pub username: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            playback: PlaybackSettings::default(),
            audio: AudioSettings::default(),
            library: LibrarySettings::default(),
            ui: UiSettings::default(),
            lastfm: LastfmSettings::default(),
            discord: DiscordSettings::default(),
            acoustid: AcoustidSettings::default(),
            smart_lists: Vec::new(),
        }
    }
}

impl Settings {
    /// Приводит значения в допустимые диапазоны. Вызывается после загрузки с
    /// диска и после каждого патча — фронт не единственный источник правды.
    pub fn normalize(&mut self) {
        self.version = SCHEMA_VERSION;
        self.playback.volume = self.playback.volume.clamp(0.0, 1.0);
        if !self.playback.volume.is_finite() {
            self.playback.volume = PlaybackSettings::default().volume;
        }

        self.audio.buffer_ms = self.audio.buffer_ms.clamp(200, 4000);
        self.audio.crossfade_ms = self.audio.crossfade_ms.min(10_000);
        self.audio.replay_gain_preamp_db = clamp_db(self.audio.replay_gain_preamp_db, 15.0);
        self.audio.eq_preamp_db = clamp_db(self.audio.eq_preamp_db, 15.0);

        if let Some(profile) = self.audio.headphone.as_mut() {
            profile.normalize();
        }
        self.audio.eq_bands_db.resize(EQ_BAND_COUNT, 0.0);
        for band in &mut self.audio.eq_bands_db {
            *band = clamp_db(*band, 15.0);
        }

        // Цвет из настроек уходит прямо в CSS, поэтому пускаем только то, что
        // точно цвет: чужая строка там превратилась бы в сломанную тему.
        if !self.ui.accent.as_deref().is_some_and(is_hex_color) {
            self.ui.accent = None;
        }
        self.ui.font_size = self.ui.font_size.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX);
        // Имя шрифта тоже уходит в CSS — пускаем только то, что похоже на имя.
        if !is_font_name(&self.ui.font) {
            self.ui.font = FONT_DEFAULT.to_owned();
        }
        if !is_font_name(&self.ui.lyrics_font) {
            self.ui.lyrics_font = LYRICS_FONT_DEFAULT.to_owned();
        }
        // Колонка названия есть всегда — без неё список не прочитать; повторы
        // выбрасываем, ширину держим в разумных пределах.
        let mut seen = std::collections::HashSet::new();
        self.ui
            .playlist_columns
            .retain(|column| seen.insert(column.id));
        if !seen.contains(&ColumnId::Title) {
            self.ui.playlist_columns = default_columns();
        }
        self.ui.sidebar_width = self.ui.sidebar_width.clamp(160, 480);
        for column in &mut self.ui.playlist_columns {
            column.width = column.width.map(|width| width.clamp(36, 900));
        }
    }
}

/// Буквы, цифры, пробел, дефис, точка и подчёркивание: кавычка или скобка в
/// имени шрифта вышла бы из строки в CSS.
fn is_font_name(value: &str) -> bool {
    let length = value.chars().count();
    (1..=64).contains(&length)
        && value
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.'))
}

fn is_hex_color(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('#') else {
        return false;
    };
    digits.len() == 6 && digits.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn clamp_db(value: f32, limit: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-limit, limit)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Цвет акцента уезжает в CSS как есть, поэтому всё, что не похоже на
    /// `#rrggbb`, до туда доходить не должно.
    #[test]
    fn keeps_only_real_colors() {
        let mut settings = Settings::default();
        for good in ["#ff8800", "#FFFFFF", "#000000"] {
            settings.ui.accent = Some(good.to_owned());
            settings.normalize();
            assert_eq!(settings.ui.accent.as_deref(), Some(good));
        }
        for bad in ["red", "#fff", "#12345g", "", "#ff8800; background: url(x)"] {
            settings.ui.accent = Some(bad.to_owned());
            settings.normalize();
            assert_eq!(settings.ui.accent, None, "{bad} не цвет");
        }
    }

    #[test]
    fn keeps_font_settings_sane() {
        let mut settings = Settings::default();
        settings.ui.font_size = 99;
        settings.ui.font = "Noto Sans".to_owned();
        settings.ui.lyrics_font = "x'; } body { display: none".to_owned();
        settings.normalize();
        assert_eq!(settings.ui.font_size, FONT_SIZE_MAX);
        assert_eq!(settings.ui.font, "Noto Sans");
        assert_eq!(settings.ui.lyrics_font, LYRICS_FONT_DEFAULT);

        settings.ui.font = String::new();
        settings.ui.font_size = 1;
        settings.normalize();
        assert_eq!(settings.ui.font, FONT_DEFAULT);
        assert_eq!(settings.ui.font_size, FONT_SIZE_MIN);
    }
}
