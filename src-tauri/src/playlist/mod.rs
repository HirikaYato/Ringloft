//! Плейлист: модель, сортировка, порядок воспроизведения.
//!
//! Живёт в Rust, а не во фронте: автопереход в конце трека, медиаклавиши и
//! трей должны работать независимо от того, что сейчас нарисовано в вебвью.

mod collection;
mod cue;
mod m3u;
mod order;
mod sources;
mod store;

pub use collection::Collection;
pub use sources::{PlaylistSources, SourcesRefresh};
pub use store::{PlaylistEvent, PlaylistStore};

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::settings::RepeatMode;

/// Расширения, которые умеет наш набор декодеров symphonia.
/// Opus, WavPack и APE сюда не входят — декодеров для них нет.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "mp2", "flac", "wav", "wave", "ogg", "oga", "opus", "weba", "m4a", "m4b", "mp4", "aac",
    "mka", "aiff", "aif",
];

/// Плейлист, а не трек: такие открываются каждый в своей вкладке.
pub fn is_list_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .is_some_and(|ext| ext == "m3u" || ext == "m3u8" || ext == "cue")
}

pub fn is_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .is_some_and(|ext| AUDIO_EXTENSIONS.contains(&ext.as_str()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItem {
    pub id: u64,
    pub path: PathBuf,
    /// Пока теги не прочитаны — имя файла без расширения.
    pub title: String,
    // Необязательные поля помечены default: файл сессии переживёт добавление
    // новых колонок без «битый json → пустой плейлист».
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub track_no: Option<u32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub meta_loaded: bool,
    /// Для треков из CUE — кусок внутри общего файла.
    #[serde(default)]
    pub start_ms: Option<u64>,
    #[serde(default)]
    pub end_ms: Option<u64>,
    /// Для настраиваемых колонок.
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub bitrate_kbps: Option<u32>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    /// Какой набор полей прочитан. Строки, прочитанные раньше, чем появились
    /// новые колонки, перечитываются один раз (`META_VERSION`).
    #[serde(default)]
    pub meta_version: u8,
}

/// Поднимать, когда строка начинает хранить новые поля из тегов.
pub const META_VERSION: u8 = 1;

/// Что прочитали из тегов для строки плейлиста.
#[derive(Debug, Clone, Default)]
pub struct ItemMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_no: Option<u32>,
    pub duration_ms: Option<u64>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
}

impl PlaylistItem {
    pub(crate) fn from_path(id: u64, path: PathBuf) -> Self {
        let title = if crate::audio::is_stream(&path) {
            // У потока нет имени файла: до первого названия из эфира
            // показываем адрес без схемы.
            let text = path.to_string_lossy();
            text.split_once("//")
                .map(|(_, rest)| rest.trim_end_matches('/').to_owned())
                .unwrap_or_else(|| text.into_owned())
        } else {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("без названия")
                .to_owned()
        };
        Self {
            id,
            path,
            title,
            artist: None,
            album: None,
            track_no: None,
            duration_ms: None,
            meta_loaded: false,
            start_ms: None,
            end_ms: None,
            year: None,
            genre: None,
            bitrate_kbps: None,
            sample_rate: None,
            meta_version: 0,
        }
    }

    /// Ссылка для движка: путь, кусок (если это трек из CUE) и название,
    /// которое движку взять больше неоткуда.
    pub fn track_ref(&self) -> crate::audio::TrackRef {
        crate::audio::TrackRef {
            item_id: self.id,
            path: self.path.clone(),
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            title: Some(self.title.clone()),
        }
    }
}

/// Строка для таблицы. Отдельный тип: во фронт уезжает `String`, а не `PathBuf`,
/// и добавляется индекс, который во фронте считать неоткуда.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlaylistRow.ts")]
pub struct PlaylistRow {
    pub index: u32,
    pub id: u64,
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_no: Option<u32>,
    pub duration_ms: Option<u64>,
    pub meta_loaded: bool,
    pub is_current: bool,
    /// Трек внутри общего файла (CUE) — для него не показываем путь как имя.
    pub is_region: bool,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    /// Из библиотеки — у файлов вне её `None`.
    pub play_count: Option<u32>,
    pub rating: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SortKey.ts")]
pub enum SortKey {
    #[default]
    Manual,
    Title,
    Artist,
    Album,
    Duration,
    TrackNo,
    Path,
    Year,
    Genre,
    Bitrate,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlaylistSort.ts")]
pub struct PlaylistSort {
    pub key: SortKey,
    pub ascending: bool,
}

impl Default for PlaylistSort {
    fn default() -> Self {
        Self {
            key: SortKey::Manual,
            ascending: true,
        }
    }
}

/// Вкладка для панели плейлистов.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlaylistTab.ts")]
pub struct PlaylistTab {
    pub id: u64,
    pub name: String,
    pub count: u32,
    pub is_active: bool,
    /// Из этого списка сейчас играет музыка — он может быть не тем, что открыт.
    pub is_playing: bool,
    /// Вкладка «Открытые файлы»: временная очередь, а не собранный список.
    pub is_temporary: bool,
    /// Сколько папок-источников у списка; 0 — собран руками.
    pub sources: u32,
}

/// Результат поиска: строки одной страницы и сколько нашлось всего.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlaylistSearch.ts")]
pub struct PlaylistSearch {
    pub total: u32,
    pub rows: Vec<PlaylistRow>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlaylistMeta.ts")]
pub struct PlaylistMeta {
    pub count: u32,
    pub current_index: Option<u32>,
    pub sort: PlaylistSort,
    /// Растёт при любом изменении: фронт по нему понимает, что окно пора перечитать.
    pub version: u64,
    pub total_duration_ms: u64,
    pub tabs: Vec<PlaylistTab>,
    /// Сколько строк ждёт своей очереди.
    pub queued: u32,
}

/// Как слушают конкретный список. Повтор и перемешивание — свойство списка,
/// а не плеера: вернувшись к плейлисту, хочется продолжить так же, как его
/// слушали в прошлый раз.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "PlaybackMode.ts")]
pub struct PlaybackMode {
    pub repeat: RepeatMode,
    pub shuffle: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    items: Vec<PlaylistItem>,
    /// Текущий трек хранится по id, а не по индексу: сортировка и удаление
    /// соседних строк не должны его сбивать.
    current: Option<u64>,
    #[serde(default)]
    sort: PlaylistSort,
    /// Запомненный режим. `None` — список ещё не играл, и режим он получит
    /// от того, что выбрано в плеере сейчас.
    #[serde(default)]
    mode: Option<PlaybackMode>,
    /// Папки, из которых собран список: по ним он и обновляется (`sources.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sources: Vec<PathBuf>,
    /// Файлы из этих папок, убранные из списка руками: обновление не должно
    /// возвращать их обратно.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    excluded: BTreeSet<PathBuf>,
    #[serde(skip)]
    shuffle_order: Vec<usize>,
    #[serde(skip)]
    version: u64,
}

impl Playlist {
    pub fn current_id(&self) -> Option<u64> {
        self.current
    }

    pub fn mode(&self) -> Option<PlaybackMode> {
        self.mode
    }

    pub fn set_mode(&mut self, mode: PlaybackMode) {
        self.mode = Some(mode);
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current.and_then(|id| self.index_of(id))
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.items.iter().position(|item| item.id == id)
    }

    pub fn item(&self, id: u64) -> Option<&PlaylistItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn at(&self, index: usize) -> Option<&PlaylistItem> {
        self.items.get(index)
    }

    pub fn items(&self) -> &[PlaylistItem] {
        &self.items
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.items
            .iter()
            .filter_map(|item| item.duration_ms)
            .sum::<u64>()
    }

    pub fn meta(&self, tabs: Vec<PlaylistTab>, queued: u32) -> PlaylistMeta {
        PlaylistMeta {
            count: self.items.len() as u32,
            current_index: self.current_index().map(|index| index as u32),
            sort: self.sort,
            version: self.version,
            total_duration_ms: self.total_duration_ms(),
            tabs,
            queued,
        }
    }

    /// Поиск по названию, исполнителю, альбому и имени файла.
    ///
    /// Слова запроса проверяются все и в любом порядке: «kishlak ночь»
    /// найдёт «Кишлак — Ночь» независимо от того, где что стоит.
    pub fn search(&self, query: &str, offset: usize, limit: usize) -> (usize, Vec<PlaylistRow>) {
        let words: Vec<String> = query
            .split_whitespace()
            .map(|word| word.to_lowercase())
            .collect();
        if words.is_empty() {
            return (0, Vec::new());
        }

        let found: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| matches_words(item, &words))
            .map(|(index, _)| index)
            .collect();

        let rows = found
            .iter()
            .skip(offset)
            .take(limit)
            .filter_map(|&index| self.rows(index, 1).into_iter().next())
            .collect();
        (found.len(), rows)
    }

    pub fn rows(&self, offset: usize, limit: usize) -> Vec<PlaylistRow> {
        let end = offset.saturating_add(limit).min(self.items.len());
        if offset >= end {
            return Vec::new();
        }
        self.items[offset..end]
            .iter()
            .enumerate()
            .map(|(nth, item)| self.row(offset + nth, item))
            .collect()
    }

    pub(crate) fn row(&self, index: usize, item: &PlaylistItem) -> PlaylistRow {
        PlaylistRow {
            index: index as u32,
            id: item.id,
            path: item.path.to_string_lossy().into_owned(),
            title: item.title.clone(),
            artist: item.artist.clone(),
            album: item.album.clone(),
            track_no: item.track_no,
            duration_ms: item.duration_ms,
            meta_loaded: item.meta_loaded,
            is_current: Some(item.id) == self.current,
            is_region: item.start_ms.is_some(),
            year: item.year,
            genre: item.genre.clone(),
            bitrate_kbps: item.bitrate_kbps,
            sample_rate: item.sample_rate,
            play_count: None,
            rating: None,
        }
    }

    /// Вставляет готовые строки (идентификаторы выдаёт коллекция —
    /// они общие на все вкладки, иначе очередь путала бы строки разных списков).
    pub fn add_items(&mut self, mut items: Vec<PlaylistItem>, at: Option<usize>) {
        if items.is_empty() {
            return;
        }
        // Вернули руками — значит, снова нужен.
        for item in &items {
            self.excluded.remove(&item.path);
        }
        match at {
            Some(index) if index < self.items.len() => {
                self.items.splice(index..index, items);
            }
            _ => self.items.append(&mut items),
        }
        self.sort = PlaylistSort::default();
        self.touch();
    }

    pub fn new_named(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            ..Self::default()
        }
    }

    /// Убрать строки по воле пользователя. Файлы из папок-источников при
    /// этом запоминаются, иначе следующее обновление вернуло бы их.
    pub fn remove(&mut self, ids: &[u64]) {
        let ids: HashSet<u64> = ids.iter().copied().collect();
        for item in self.items.iter().filter(|item| ids.contains(&item.id)) {
            if self.is_under_source(&item.path) {
                self.excluded.insert(item.path.clone());
            }
        }
        self.forget(&ids);
    }

    /// Убрать строки, потому что файла больше нет: запоминать их незачем.
    pub(crate) fn forget(&mut self, ids: &HashSet<u64>) {
        if ids.is_empty() {
            return;
        }
        self.items.retain(|item| !ids.contains(&item.id));
        if self.current.is_some_and(|id| ids.contains(&id)) {
            self.current = None;
        }
        self.touch();
    }

    /// Очищенный список собирают заново, поэтому и источники уходят: иначе
    /// обновление при запуске наполнило бы его обратно.
    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
        self.sources.clear();
        self.excluded.clear();
        self.touch();
    }

    pub fn set_current(&mut self, id: Option<u64>) {
        if self.current != id {
            self.current = id;
            // Именно bump_version, а не touch: смена текущего трека не должна
            // пересобирать перемешанный порядок, иначе шаффл начнёт повторяться.
            self.bump_version();
        }
    }

    pub fn update_meta(&mut self, id: u64, meta: ItemMeta) {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return;
        };
        item.meta_version = META_VERSION;
        if item.start_ms.is_some() {
            // Треку из CUE теги общего файла не подходят: у него своё название,
            // свой исполнитель и своя длительность из листа.
            item.meta_loaded = true;
            self.version += 1;
            return;
        }
        if let Some(title) = meta.title.filter(|value| !value.trim().is_empty()) {
            item.title = title;
        }
        item.artist = meta.artist;
        item.album = meta.album;
        item.track_no = meta.track_no;
        item.duration_ms = meta.duration_ms;
        item.year = meta.year;
        item.genre = meta.genre;
        item.bitrate_kbps = meta.bitrate_kbps;
        item.sample_rate = meta.sample_rate;
        item.meta_loaded = true;
        self.version += 1;
    }

    /// Подменяет путь у строк после переименования файла. Строку не
    /// пересоздаём: у неё уже прочитаны теги, а у куска CUE ещё и границы.
    pub(crate) fn replace_paths(&mut self, pairs: &[(PathBuf, PathBuf)]) -> usize {
        let mut touched = 0;
        for item in self.items.iter_mut() {
            let Some((from, to)) = pairs.iter().find(|(from, _)| *from == item.path) else {
                continue;
            };
            // Название, взятое из имени файла (тегов не было), тоже обновляем:
            // иначе в списке останется прежнее имя.
            let from_stem = from
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem == item.title);
            item.path = to.clone();
            if from_stem {
                item.title = to
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or(&item.title)
                    .to_owned();
            }
            touched += 1;
        }
        if touched > 0 {
            self.bump_version();
        }
        touched
    }

    /// Новый круг перемешивания: он соберётся заново при следующем обращении
    /// и начнётся с текущего трека.
    pub(crate) fn restart_shuffle(&mut self) {
        self.shuffle_order.clear();
    }

    fn touch(&mut self) {
        self.version += 1;
        self.shuffle_order.clear();
    }

    fn bump_version(&mut self) {
        self.version += 1;
    }
}

/// Строка подходит, если содержит все слова запроса. Регистр приводим на
/// месте: список бывает на пятьдесят тысяч строк, но поиск идёт по нажатию
/// клавиши с задержкой, и заранее построенный индекс тут не окупается.
fn matches_words(item: &PlaylistItem, words: &[String]) -> bool {
    let fields = [
        Some(item.title.to_lowercase()),
        item.artist.as_ref().map(|value| value.to_lowercase()),
        item.album.as_ref().map(|value| value.to_lowercase()),
        item.path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase()),
    ];
    words.iter().all(|word| {
        fields
            .iter()
            .flatten()
            .any(|field| field.contains(word.as_str()))
    })
}
