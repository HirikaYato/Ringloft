//! Состояние плейлистов: вкладки, очередь, фоновое чтение тегов, сессия.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use super::{
    ItemMeta, META_VERSION,
    Collection, PlaybackMode, PlaylistMeta, PlaylistRow, SortKey, cue, is_audio_path, m3u,
};
use crate::atomic_file::{self, DebouncedWriter};
use crate::audio::TrackRef;
use crate::error::{RingloftError, Result};
use crate::settings::RepeatMode;
use crate::tags;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(700);
/// Как часто фоновое чтение тегов дёргает фронт.
const NOTIFY_PERIOD: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy)]
pub enum PlaylistEvent {
    /// Что-то изменилось — фронту пора перечитать мету и видимое окно.
    Changed,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    #[serde(default)]
    collection: Collection,
    #[serde(default)]
    position_ms: u64,
}

pub struct PlaylistStore {
    pub(super) inner: RwLock<Collection>,
    writer: DebouncedWriter,
    events: Sender<PlaylistEvent>,
    pub(super) meta_tx: Sender<u64>,
    restored_position_ms: u64,
    position_ms: AtomicU64,
}

impl PlaylistStore {
    pub fn load(path: PathBuf) -> (Arc<Self>, Receiver<PlaylistEvent>) {
        let mut session: Session = atomic_file::read_json(&path).unwrap_or_default();
        session.collection.normalize();

        let (events, events_rx) = crossbeam_channel::unbounded();
        let (meta_tx, meta_rx) = crossbeam_channel::unbounded();

        let store = Arc::new(Self {
            inner: RwLock::new(session.collection),
            writer: DebouncedWriter::new(path, "ringloft-playlist", SAVE_DEBOUNCE),
            events,
            meta_tx,
            restored_position_ms: session.position_ms,
            position_ms: AtomicU64::new(session.position_ms),
        });

        spawn_meta_worker(Arc::downgrade(&store), meta_rx);
        store.queue_missing_meta();
        (store, events_rx)
    }

    pub fn restored_position_ms(&self) -> u64 {
        self.restored_position_ms
    }

    pub fn set_position_ms(&self, position_ms: u64) {
        self.position_ms.store(position_ms, Ordering::Relaxed);
    }

    pub fn request_save(&self) {
        self.schedule_save();
    }

    pub fn meta(&self) -> PlaylistMeta {
        let collection = self.inner.read();
        let tabs = collection.tabs();
        let queued = collection.queued() as u32;
        collection.active().meta(tabs, queued)
    }

    pub fn rows(&self, offset: usize, limit: usize) -> Vec<PlaylistRow> {
        self.inner.read().active().rows(offset, limit)
    }

    /// Уникальные файлы открытой вкладки. Треки CUE делят один файл, поэтому
    /// путь берём по одному разу.
    pub fn paths(&self) -> Vec<PathBuf> {
        let collection = self.inner.read();
        let mut seen = std::collections::HashSet::new();
        collection
            .active()
            .items()
            .iter()
            .filter(|item| seen.insert(item.path.clone()))
            .map(|item| item.path.clone())
            .collect()
    }

    /// Новый круг перемешивания у играющей вкладки — с её текущего трека.
    pub fn restart_shuffle(&self) {
        let mut collection = self.inner.write();
        let playing = collection.playing_id();
        if let Some(list) = collection.get_mut(playing) {
            list.restart_shuffle();
        }
    }

    pub fn playing_tab_id(&self) -> u64 {
        self.inner.read().playing_id()
    }

    /// Режим, запомненный играющей вкладкой. `None` — она ещё не играла.
    pub fn playing_mode(&self) -> Option<PlaybackMode> {
        let collection = self.inner.read();
        let id = collection.playing_id();
        collection.get(id).and_then(|list| list.mode())
    }

    /// Запомнить режим за играющей вкладкой.
    pub fn remember_mode(&self, mode: PlaybackMode) {
        {
            let mut collection = self.inner.write();
            let id = collection.playing_id();
            let Some(list) = collection.get_mut(id) else {
                return;
            };
            if list.mode() == Some(mode) {
                return;
            }
            list.set_mode(mode);
        }
        self.request_save();
    }

    /// Что заиграет дальше — из играющей вкладки, а не из открытой.
    pub fn upcoming_rows(
        &self,
        repeat: RepeatMode,
        shuffle: bool,
        limit: usize,
    ) -> Vec<PlaylistRow> {
        let mut collection = self.inner.write();
        let playing_id = collection.playing_id();
        let Some(list) = collection.get_mut(playing_id) else {
            return Vec::new();
        };
        let ids = list.upcoming_ids(repeat, shuffle, limit);
        let Some(list) = collection.get(playing_id) else {
            return Vec::new();
        };
        ids.into_iter()
            .filter_map(|id| list.index_of(id))
            .filter_map(|index| list.rows(index, 1).into_iter().next())
            .collect()
    }

    /// Поиск по открытой вкладке.
    pub fn search(&self, query: &str, offset: usize, limit: usize) -> (usize, Vec<PlaylistRow>) {
        self.inner.read().active().search(query, offset, limit)
    }

    /// Строки очереди — в порядке воспроизведения.
    pub fn queue_rows(&self) -> Vec<PlaylistRow> {
        let collection = self.inner.read();
        collection
            .queue_entries()
            .filter_map(|entry| {
                collection
                    .get(entry.playlist_id)
                    .and_then(|list| list.index_of(entry.item_id).map(|index| (list, index)))
                    .and_then(|(list, index)| list.rows(index, 1).into_iter().next())
            })
            .collect()
    }

    /// Пути выбранных строк открытой вкладки. Куски CUE пропускаем (теги у
    /// них общие с файлом-образом), радио — тоже: править нечего.
    pub fn paths_at(&self, indexes: &[usize]) -> Vec<PathBuf> {
        let collection = self.inner.read();
        let items = collection.active().items();
        let mut paths = Vec::with_capacity(indexes.len());
        for &index in indexes {
            if let Some(item) = items.get(index)
                && item.start_ms.is_none()
                && !crate::audio::is_stream(&item.path)
                && !paths.contains(&item.path)
            {
                paths.push(item.path.clone());
            }
        }
        paths
    }

    /// Перечитать теги у строк с этими путями. Нужно после правки тегов:
    /// файл изменился, а в списке лежат прежние значения.
    pub fn refresh_meta(&self, paths: &[PathBuf]) {
        let ids: Vec<u64> = {
            let collection = self.inner.read();
            collection
                .all_items()
                .filter(|item| paths.iter().any(|path| path == &item.path))
                .map(|item| item.id)
                .collect()
        };
        for id in ids {
            let _ = self.meta_tx.send(id);
        }
    }

    /// Ссылка на трек по идентификатору строки.
    pub fn track_of(&self, id: u64) -> Option<TrackRef> {
        self.inner.read().find_item(id).map(|(_, item)| item.track_ref())
    }

    pub fn path_of(&self, id: u64) -> Option<PathBuf> {
        self.inner
            .read()
            .find_item(id)
            .map(|(_, item)| item.path.clone())
    }

    pub fn id_at(&self, index: usize) -> Option<u64> {
        self.inner.read().active().at(index).map(|item| item.id)
    }

    pub fn current_id(&self) -> Option<u64> {
        let collection = self.inner.read();
        let playing = collection.playing_id();
        collection.get(playing).and_then(|list| list.current_id())
    }

    pub fn item_summary(&self, id: u64) -> Option<(String, Option<String>, Option<String>)> {
        self.inner.read().find_item(id).map(|(_, item)| {
            (
                item.title.clone(),
                item.artist.clone(),
                item.album.clone(),
            )
        })
    }

    // --- вкладки ---

    pub fn create_tab(&self, name: Option<String>) -> u64 {
        let id = self.inner.write().create(name);
        self.changed();
        id
    }

    pub fn rename_tab(&self, id: u64, name: String) {
        self.inner.write().rename(id, name);
        self.changed();
    }

    pub fn move_tab(&self, id: u64, before: Option<u64>) {
        self.inner.write().move_tab(id, before);
        self.changed();
    }

    pub fn close_tab(&self, id: u64) {
        self.inner.write().close(id);
        self.changed();
    }

    pub fn activate_tab(&self, id: u64) {
        self.inner.write().set_active(id);
        self.changed();
    }

    // --- содержимое активной вкладки ---

    /// Добавленные папки становятся источниками списка: по ним он потом
    /// обновляется (`sources.rs`).
    pub fn add_paths(&self, paths: &[PathBuf], at: Option<usize>) -> usize {
        let files = expand_paths(paths);
        if files.is_empty() {
            return 0;
        }

        let ids = {
            let mut collection = self.inner.write();
            let items = collection.build_items(files);
            let ids: Vec<u64> = items.iter().map(|item| item.id).collect();
            let list = collection.active_mut();
            for dir in paths.iter().filter(|path| path.is_dir()) {
                list.add_source(dir.clone());
            }
            list.add_items(items, at);
            ids
        };

        let added = ids.len();
        for id in ids {
            let _ = self.meta_tx.send(id);
        }
        self.changed();
        added
    }

    /// Файлы, открытые снаружи (проводник, вторая копия, MPRIS): своя
    /// вкладка вместо активной. Возвращает первую строку — её и играть.
    pub fn open_external(&self, paths: &[PathBuf]) -> Option<u64> {
        let files = expand_paths(paths);
        if files.is_empty() {
            return None;
        }

        let ids = {
            let mut collection = self.inner.write();
            let items = collection.build_items(files);
            let ids: Vec<u64> = items.iter().map(|item| item.id).collect();
            collection.fill_opened(items);
            ids
        };

        let first = ids.first().copied();
        for id in ids {
            let _ = self.meta_tx.send(id);
        }
        self.changed();
        first
    }

    pub fn remove(&self, ids: &[u64]) {
        {
            let mut collection = self.inner.write();
            collection.active_mut().remove(ids);
            for id in ids {
                collection.remove_from_queue(*id);
            }
        }
        self.changed();
    }

    pub fn clear(&self) {
        self.inner.write().active_mut().clear();
        self.changed();
    }

    /// Убирает из списка строки, файлов которых больше нет. Радио не трогаем:
    /// у него путь — это адрес, и на диске его быть не должно.
    pub fn drop_missing(&self) -> usize {
        let gone: Vec<PathBuf> = {
            let collection = self.inner.read();
            collection
                .active()
                .items()
                .iter()
                .filter(|item| {
                    !crate::audio::is_stream(&item.path) && !item.path.is_file()
                })
                .map(|item| item.path.clone())
                .collect()
        };
        self.drop_paths(&gone)
    }

    /// Выкидывает из всех вкладок строки с этими путями: файла больше нет.
    pub fn drop_paths(&self, paths: &[PathBuf]) -> usize {
        let dropped = self.inner.write().drop_paths(paths);
        if dropped > 0 {
            self.changed();
        }
        dropped
    }

    /// Меняет путь у строк после переименования файла. Строку не пересоздаём:
    /// у неё уже прочитаны теги, а у куска CUE ещё и границы.
    pub fn replace_paths(&self, pairs: &[(PathBuf, PathBuf)]) {
        let touched = self.inner.write().replace_paths(pairs);
        if touched > 0 {
            self.changed();
        }
    }

    /// Перенос строк в другую вкладку. `copy` — оставить и в этой.
    pub fn transfer(&self, indexes: &[usize], target: u64, copy: bool) -> usize {
        let moved = self.inner.write().transfer(indexes, target, copy);
        if moved > 0 {
            self.changed();
        }
        moved
    }

    pub fn move_items(&self, ids: &[u64], to_index: usize) {
        self.inner.write().active_mut().move_items(ids, to_index);
        self.changed();
    }

    pub fn sort_by(&self, key: SortKey, ascending: bool) {
        self.inner.write().active_mut().sort_by(key, ascending);
        self.changed();
    }

    // --- M3U ---

    /// Импортирует список в новую вкладку, названную по имени файла.
    /// Возвращает, сколько строк добавилось и сколько было в файле.
    pub fn import_m3u(&self, path: &Path) -> Result<(usize, usize)> {
        let bytes = std::fs::read(path).map_err(|err| RingloftError::io(path, err))?;
        let content = crate::text::decode(bytes);
        let base = path.parent().unwrap_or(Path::new("."));
        let listed = m3u::parse(&content, base);

        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Импорт")
            .to_owned();
        self.create_tab(Some(name));

        let added = self.add_paths(&listed, None);
        Ok((added, listed.len()))
    }

    /// Импортирует CUE: один образ — несколько треков с отступами.
    /// Возвращает, сколько треков добавилось и сколько было в листе.
    pub fn import_cue(&self, path: &Path) -> Result<(usize, usize)> {
        let bytes = std::fs::read(path).map_err(|err| RingloftError::io(path, err))?;
        let content = crate::text::decode(bytes);
        let base = path.parent().unwrap_or(Path::new("."));
        let sheet = cue::parse(&content, base);

        let listed = sheet.tracks.len();
        let tracks: Vec<&cue::CueTrack> = sheet
            .tracks
            .iter()
            .filter(|track| track.file.is_file())
            .collect();

        if tracks.is_empty() {
            return Ok((0, listed));
        }

        // Конец последнего трека каждого файла — это конец самого файла.
        let mut lengths: std::collections::HashMap<PathBuf, u64> =
            std::collections::HashMap::new();
        for track in &tracks {
            if track.end_ms.is_none() && !lengths.contains_key(&track.file) {
                let duration = tags::read(&track.file)
                    .map(|tags| tags.duration_ms)
                    .unwrap_or(0);
                lengths.insert(track.file.clone(), duration);
            }
        }

        let name = sheet.album.clone().unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("CUE")
                .to_owned()
        });
        self.create_tab(Some(name));

        {
            let mut collection = self.inner.write();
            let paths = tracks.iter().map(|track| track.file.clone()).collect();
            let mut items = collection.build_items(paths);

            for (item, track) in items.iter_mut().zip(&tracks) {
                let end_ms = track.end_ms.or_else(|| {
                    lengths
                        .get(&track.file)
                        .copied()
                        .filter(|length| *length > track.start_ms)
                });

                item.title = track.title.clone();
                item.artist = track.artist.clone().or_else(|| sheet.performer.clone());
                item.album = sheet.album.clone();
                item.track_no = Some(track.number);
                item.start_ms = Some(track.start_ms);
                item.end_ms = end_ms;
                item.duration_ms = end_ms.map(|end| end.saturating_sub(track.start_ms));
                // Теги общего файла такому треку не подходят — всё уже из листа.
                item.meta_loaded = true;
            }

            collection.active_mut().add_items(items, None);
        }

        self.changed();
        Ok((tracks.len(), listed))
    }

    /// Сохраняет активную вкладку в M3U8.
    pub fn export_m3u(&self, path: &Path) -> Result<usize> {
        let target_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let text = {
            let collection = self.inner.read();
            let items: Vec<&super::PlaylistItem> = collection.active().items().iter().collect();
            m3u::render(&items, &target_dir)
        };
        let count = text.lines().filter(|line| !line.starts_with('#')).count();
        atomic_file::write_atomic(path, &text)?;
        Ok(count)
    }

    // --- очередь ---

    pub fn enqueue(&self, ids: &[u64]) {
        {
            let mut collection = self.inner.write();
            let active = collection.active_id();
            collection.enqueue(active, ids);
        }
        self.changed();
    }

    pub fn clear_queue(&self) {
        self.inner.write().clear_queue();
        self.changed();
    }

    // --- воспроизведение ---

    /// Пользователь выбрал строку: эта вкладка становится играющей.
    pub fn play_item(&self, item_id: u64) -> Option<TrackRef> {
        let track = {
            let mut collection = self.inner.write();
            let (playlist_id, track) = collection
                .find_item(item_id)
                .map(|(playlist_id, item)| (playlist_id, item.track_ref()))?;
            collection.set_playing(playlist_id);
            collection.remove_from_queue(item_id);
            if let Some(list) = collection.get_mut(playlist_id) {
                list.set_current(Some(item_id));
            }
            track
        };
        self.changed();
        Some(track)
    }

    /// Что будет следующим, не трогая ни текущий трек, ни очередь.
    pub fn peek_next(&self, repeat: RepeatMode, shuffle: bool) -> Option<TrackRef> {
        let mut collection = self.inner.write();
        let (_, item_id, _) = collection.next_playback(repeat, shuffle, false, false)?;
        collection.find_item(item_id).map(|(_, item)| item.track_ref())
    }

    /// Следующий трек: помечает его текущим и отдаёт путь для загрузки.
    pub fn advance(&self, repeat: RepeatMode, shuffle: bool, manual: bool) -> Option<TrackRef> {
        let picked = {
            let mut collection = self.inner.write();
            let (playlist_id, item_id, _) =
                collection.next_playback(repeat, shuffle, manual, true)?;
            collection.set_playing(playlist_id);
            if let Some(list) = collection.get_mut(playlist_id) {
                list.set_current(Some(item_id));
            }
            collection.find_item(item_id).map(|(_, item)| item.track_ref())
        };
        self.changed();
        picked
    }

    pub fn go_back(&self, shuffle: bool) -> Option<TrackRef> {
        let picked = {
            let mut collection = self.inner.write();
            let (playlist_id, item_id, _) = collection.prev_playback(shuffle)?;
            collection.set_playing(playlist_id);
            if let Some(list) = collection.get_mut(playlist_id) {
                list.set_current(Some(item_id));
            }
            collection.find_item(item_id).map(|(_, item)| item.track_ref())
        };
        self.changed();
        picked
    }

    /// Движок сообщил, что заиграл трек: плейлист догоняет и убирает его из очереди.
    /// Идём по идентификатору строки, а не по пути: у треков из CUE путь общий.
    pub fn set_current_by_item(&self, item_id: u64) -> Option<u64> {
        let playlist_id = self.inner.read().find_item(item_id).map(|(id, _)| id)?;

        {
            let mut collection = self.inner.write();
            collection.set_playing(playlist_id);
            collection.remove_from_queue(item_id);
            if let Some(list) = collection.get_mut(playlist_id) {
                list.set_current(Some(item_id));
            }
        }
        self.changed();
        Some(item_id)
    }

    /// Синхронное сохранение вместе с позицией — на выходе из приложения.
    pub fn save_with_position(&self, position_ms: u64) -> Result<()> {
        self.set_position_ms(position_ms);
        let json = {
            let collection = self.inner.read();
            serde_json::to_string(&SessionRef {
                collection: &collection,
                position_ms,
            })?
        };
        self.writer.flush_now(&json)
    }

    fn queue_missing_meta(&self) {
        let ids: Vec<u64> = {
            let collection = self.inner.read();
            // Не прочитанные и прочитанные раньше, чем появились нужные
            // колонки (год, жанр, битрейт), — один раз в фоне.
            collection
                .all_items()
                .filter(|item| !item.meta_loaded || item.meta_version < META_VERSION)
                .map(|item| item.id)
                .collect()
        };
        for id in ids {
            let _ = self.meta_tx.send(id);
        }
    }

    pub(super) fn changed(&self) {
        let _ = self.events.send(PlaylistEvent::Changed);
        self.schedule_save();
    }

    fn schedule_save(&self) {
        let json = {
            let collection = self.inner.read();
            serde_json::to_string(&SessionRef {
                collection: &collection,
                position_ms: self.position_ms.load(Ordering::Relaxed),
            })
        };
        match json {
            Ok(json) => self.writer.schedule(json),
            Err(err) => tracing::error!(%err, "не удалось сериализовать плейлисты"),
        }
    }

    /// Читает теги одного файла. Блокировка на время ввода-вывода не держится.
    fn load_meta(&self, id: u64) -> bool {
        let Some(path) = self.path_of(id) else {
            return false;
        };
        // У радио нет файла: название придёт из потока, а не из тегов.
        if crate::audio::is_stream(&path) {
            self.update_meta(id, ItemMeta::default());
            return true;
        }
        let tags = match tags::read(&path) {
            Ok(tags) => tags,
            Err(err) => {
                tracing::debug!(%err, path = %path.display(), "теги не прочитались");
                // Всё равно помечаем строку обработанной, иначе будем
                // долбиться в один и тот же битый файл при каждом запуске.
                self.update_meta(id, ItemMeta::default());
                return true;
            }
        };
        self.update_meta(
            id,
            ItemMeta {
                title: tags.title,
                artist: tags.artist.or(tags.album_artist),
                album: tags.album,
                track_no: tags.track,
                duration_ms: Some(tags.duration_ms).filter(|value| *value > 0),
                year: tags.year,
                genre: tags.genre,
                bitrate_kbps: tags.bitrate_kbps,
                sample_rate: tags.sample_rate,
            },
        );
        true
    }

    fn update_meta(&self, id: u64, meta: ItemMeta) {
        let mut collection = self.inner.write();
        for list in collection.playlists_mut() {
            if list.item(id).is_some() {
                list.update_meta(id, meta);
                return;
            }
        }
    }
}

/// Сериализация без клонирования коллекции.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionRef<'a> {
    collection: &'a Collection,
    position_ms: u64,
}

fn spawn_meta_worker(store: Weak<PlaylistStore>, meta_rx: Receiver<u64>) {
    let spawned = std::thread::Builder::new()
        .name("ringloft-tags".into())
        .spawn(move || {
            let mut dirty = false;
            let mut last_notify = Instant::now();
            loop {
                match meta_rx.recv_timeout(NOTIFY_PERIOD) {
                    Ok(id) => {
                        let Some(store) = store.upgrade() else { break };
                        dirty |= store.load_meta(id);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }

                if dirty && last_notify.elapsed() >= NOTIFY_PERIOD {
                    let Some(store) = store.upgrade() else { break };
                    let _ = store.events.send(PlaylistEvent::Changed);
                    store.schedule_save();
                    dirty = false;
                    last_notify = Instant::now();
                }
            }
            tracing::debug!("поток чтения тегов остановлен");
        });

    if let Err(err) = spawned {
        tracing::error!(%err, "не удалось запустить поток чтения тегов");
    }
}

/// Папки разворачиваются рекурсивно, файлы фильтруются по расширению,
/// адреса потоков проходят как есть: ни расширения, ни файла у них нет.
fn expand_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if crate::audio::is_stream(path) {
            match crate::audio::clean_stream(path) {
                Some(url) => files.push(url),
                None => tracing::warn!(path = %path.display(), "адрес потока не разобрать"),
            }
        } else if path.is_dir() {
            collect_dir(path, &mut files);
        } else if is_audio_path(path) && path.is_file() {
            files.push(path.clone());
        }
    }
    files
}

pub(super) fn collect_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                tracing::debug!(%err, "папка пропущена");
                None
            }
        });

    for entry in walker {
        let path = entry.path();
        if entry.file_type().is_file() && is_audio_path(path) {
            files.push(path.to_path_buf());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("ringloft-m3u-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("временная папка");
        path
    }

    #[test]
    fn imports_list_into_its_own_tab_and_exports_back() {
        let dir = temp_dir("roundtrip");
        // Содержимое не важно: путь до файла проверяется по расширению,
        // а теги дочитает фоновый поток.
        std::fs::write(dir.join("one.flac"), b"").expect("файл");
        std::fs::write(dir.join("two.flac"), b"").expect("файл");
        std::fs::write(
            dir.join("list.m3u8"),
            "#EXTM3U\n#EXTINF:1,Первый\none.flac\n#EXTINF:2,Второй\ntwo.flac\nнету.flac\n",
        )
        .expect("список");

        let (store, _events) = PlaylistStore::load(dir.join("session.json"));
        let (added, listed) = store.import_m3u(&dir.join("list.m3u8")).expect("импорт");
        assert_eq!((added, listed), (2, 3), "пропавший файл не считается добавленным");

        let meta = store.meta();
        assert_eq!(meta.count, 2);
        assert_eq!(
            meta.tabs.iter().find(|tab| tab.is_active).map(|tab| tab.name.as_str()),
            Some("list"),
            "вкладка называется по имени файла"
        );

        let exported = dir.join("out.m3u8");
        let written = store.export_m3u(&exported).expect("экспорт");
        assert_eq!(written, 2);

        let text = std::fs::read_to_string(&exported).expect("чтение");
        assert!(text.starts_with("#EXTM3U"));
        assert!(text.contains("\none.flac\n"), "пути внутри папки — относительные");
        assert!(text.contains("\ntwo.flac\n"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
