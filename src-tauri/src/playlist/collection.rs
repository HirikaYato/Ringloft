//! Несколько плейлистов и очередь воспроизведения.
//!
//! Видимая вкладка и та, из которой играет музыка, — разные вещи: переключение
//! вкладки не должно останавливать или перебрасывать воспроизведение. Очередь
//! живёт поверх всех списков и всегда перебивает обычный порядок.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Playlist, PlaylistItem, PlaylistTab};
use crate::settings::RepeatMode;

/// Ссылка на строку: очередь может смешивать треки из разных вкладок.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntry {
    pub playlist_id: u64,
    pub item_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    playlists: Vec<Playlist>,
    /// Открытая вкладка.
    active: u64,
    /// Список, из которого идёт воспроизведение.
    playing: u64,
    #[serde(default)]
    queue: VecDeque<QueueEntry>,
    next_item_id: u64,
    next_playlist_id: u64,
    /// Вкладка для файлов, открытых из проводника. Узнаём её по номеру, а не
    /// по названию: вкладку могут переименовать.
    #[serde(default)]
    opened: Option<u64>,
}

impl Default for Collection {
    fn default() -> Self {
        let first = Playlist::new_named(1, "Плейлист".to_owned());
        Self {
            playlists: vec![first],
            active: 1,
            playing: 1,
            queue: VecDeque::new(),
            next_item_id: 0,
            next_playlist_id: 1,
            opened: None,
        }
    }
}

impl Collection {
    /// Подстраховка после чтения файла сессии: список не должен остаться пустым,
    /// а активная и играющая вкладки — указывать в никуда.
    pub fn normalize(&mut self) {
        if self.playlists.is_empty() {
            *self = Self::default();
            return;
        }
        self.next_playlist_id = self
            .next_playlist_id
            .max(self.playlists.iter().map(|list| list.id).max().unwrap_or(1));
        let first = self.playlists[0].id;
        if !self.playlists.iter().any(|list| list.id == self.active) {
            self.active = first;
        }
        if !self.playlists.iter().any(|list| list.id == self.playing) {
            self.playing = self.active;
        }
        self.queue
            .retain(|entry| self.playlists.iter().any(|list| list.id == entry.playlist_id));
        if self
            .opened
            .is_some_and(|id| !self.playlists.iter().any(|list| list.id == id))
        {
            self.opened = None;
        }
    }

    pub fn tabs(&self) -> Vec<PlaylistTab> {
        self.playlists
            .iter()
            .map(|list| PlaylistTab {
                id: list.id,
                name: list.name.clone(),
                count: list.items().len() as u32,
                is_active: list.id == self.active,
                is_playing: list.id == self.playing,
                is_temporary: self.opened == Some(list.id),
                sources: list.sources().len() as u32,
            })
            .collect()
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    pub fn active_id(&self) -> u64 {
        self.active
    }

    pub fn playing_id(&self) -> u64 {
        self.playing
    }

    pub fn active(&self) -> &Playlist {
        self.get(self.active).unwrap_or(&self.playlists[0])
    }

    pub fn active_mut(&mut self) -> &mut Playlist {
        let index = self.index_of(self.active);
        &mut self.playlists[index]
    }

    /// Индекс вкладки по id; коллекция никогда не бывает пустой, поэтому
    /// потерянный id безопасно сводится к первой вкладке.
    fn index_of(&self, id: u64) -> usize {
        self.playlists
            .iter()
            .position(|list| list.id == id)
            .unwrap_or(0)
    }

    /// Ищет строку по её идентификатору во всех вкладках.
    pub fn find_item(&self, item_id: u64) -> Option<(u64, &PlaylistItem)> {
        self.playlists
            .iter()
            .find_map(|list| list.item(item_id).map(|item| (list.id, item)))
    }

    pub fn playlists_mut(&mut self) -> impl Iterator<Item = &mut Playlist> {
        self.playlists.iter_mut()
    }

    pub fn all_items(&self) -> impl Iterator<Item = &PlaylistItem> {
        self.playlists.iter().flat_map(|list| list.items().iter())
    }


    pub fn get(&self, id: u64) -> Option<&Playlist> {
        self.playlists.iter().find(|list| list.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Playlist> {
        self.playlists.iter_mut().find(|list| list.id == id)
    }

    pub fn set_active(&mut self, id: u64) {
        if self.playlists.iter().any(|list| list.id == id) {
            self.active = id;
        }
    }

    pub fn set_playing(&mut self, id: u64) {
        if self.playlists.iter().any(|list| list.id == id) {
            self.playing = id;
        }
    }

    pub fn create(&mut self, name: Option<String>) -> u64 {
        self.next_playlist_id += 1;
        let id = self.next_playlist_id;
        let name = name.unwrap_or_else(|| format!("Плейлист {}", self.playlists.len() + 1));
        self.playlists.push(Playlist::new_named(id, name));
        self.active = id;
        id
    }

    pub fn rename(&mut self, id: u64, name: String) {
        if let Some(list) = self.get_mut(id) {
            list.name = name;
        }
    }

    /// Закрывает вкладку. Последнюю не отдаём: плееру нужен хоть один список.
    pub fn close(&mut self, id: u64) {
        if self.playlists.len() <= 1 {
            return;
        }
        self.playlists.retain(|list| list.id != id);
        self.queue.retain(|entry| entry.playlist_id != id);
        self.normalize();
    }

    /// Переставляет вкладку перед `before` (`None` — в конец). Соседом, а не
    /// индексом: временная вкладка в интерфейсе всегда последняя, а в
    /// сохранённом порядке может стоять где угодно, и индексы бы разъехались.
    pub fn move_tab(&mut self, id: u64, before: Option<u64>) {
        if before == Some(id) {
            return;
        }
        let Some(from) = self.playlists.iter().position(|list| list.id == id) else {
            return;
        };
        let list = self.playlists.remove(from);
        let to = before
            .and_then(|other| self.playlists.iter().position(|list| list.id == other))
            .unwrap_or(self.playlists.len());
        self.playlists.insert(to, list);
    }

    /// Кладёт файлы, открытые из проводника, в отдельную вкладку и возвращает
    /// её номер.
    ///
    /// В активный список они не попадают: открыть трек двойным щелчком —
    /// значит послушать его, а не дописать в собранный руками плейлист.
    /// Вкладка одна на все такие открытия и каждый раз собирается заново —
    /// это очередь «вот эти файлы», а не копилка всего, что когда-то
    /// открывали. Открытая на экране вкладка при этом не меняется.
    pub fn fill_opened(&mut self, items: Vec<PlaylistItem>) -> u64 {
        let id = match self.opened.filter(|id| self.get(*id).is_some()) {
            Some(id) => id,
            None => {
                self.next_playlist_id += 1;
                let id = self.next_playlist_id;
                self.playlists
                    .push(Playlist::new_named(id, "Открытые файлы".to_owned()));
                self.opened = Some(id);
                id
            }
        };

        // Прежние строки уходят — и записи очереди, которые на них указывали.
        self.queue.retain(|entry| entry.playlist_id != id);
        if let Some(list) = self.get_mut(id) {
            list.clear();
            list.add_items(items, None);
        }
        id
    }

    /// Создаёт строки с общими для всей коллекции идентификаторами.
    pub fn build_items(&mut self, paths: Vec<PathBuf>) -> Vec<PlaylistItem> {
        paths
            .into_iter()
            .map(|path| {
                self.next_item_id += 1;
                PlaylistItem::from_path(self.next_item_id, path)
            })
            .collect()
    }

    /// Выкидывает строки с этими путями из всех вкладок: файла больше нет.
    pub fn drop_paths(&mut self, paths: &[PathBuf]) -> usize {
        let paths: HashSet<&PathBuf> = paths.iter().collect();
        let ids: HashSet<u64> = self
            .all_items()
            .filter(|item| paths.contains(&item.path))
            .map(|item| item.id)
            .collect();
        if ids.is_empty() {
            return 0;
        }
        for list in self.playlists.iter_mut() {
            list.forget(&ids);
        }
        self.queue.retain(|entry| !ids.contains(&entry.item_id));
        ids.len()
    }

    /// Забывает строки одной вкладки вместе с их записями в очереди.
    pub fn forget(&mut self, playlist_id: u64, ids: &HashSet<u64>) {
        if let Some(list) = self.get_mut(playlist_id) {
            list.forget(ids);
        }
        self.queue.retain(|entry| !ids.contains(&entry.item_id));
    }

    /// Подменяет путь у строк после переименования файла.
    pub fn replace_paths(&mut self, pairs: &[(PathBuf, PathBuf)]) -> usize {
        self.playlists
            .iter_mut()
            .map(|list| list.replace_paths(pairs))
            .sum()
    }

    /// Переносит выбранные строки открытой вкладки в другую.
    ///
    /// `copy` — оставить их и здесь. Возвращает, сколько строк уехало.
    /// Строки переносятся целиком, а не путями: у них уже прочитаны теги, а
    /// у треков CUE ещё и границы куска — заново это не собрать.
    pub fn transfer(&mut self, indexes: &[usize], target: u64, copy: bool) -> usize {
        if target == self.active || !self.playlists.iter().any(|list| list.id == target) {
            return 0;
        }

        let mut sorted = indexes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        let picked: Vec<PlaylistItem> = {
            let source = self.active();
            sorted
                .iter()
                .filter_map(|&index| source.at(index).cloned())
                .collect()
        };
        if picked.is_empty() {
            return 0;
        }

        let ids: Vec<u64> = picked.iter().map(|item| item.id).collect();
        // Играющую строку переносим вместе с отметкой: иначе музыка идёт из
        // вкладки, в которой этой строки больше нет.
        let playing_moved = !copy
            && self.playing == self.active
            && self.active().current_id().is_some_and(|id| ids.contains(&id));
        let playing_id = self.active().current_id();

        let items: Vec<PlaylistItem> = if copy {
            // Номера строк общие на всю коллекцию: копии нужны свои.
            picked
                .into_iter()
                .map(|mut item| {
                    self.next_item_id += 1;
                    item.id = self.next_item_id;
                    item
                })
                .collect()
        } else {
            picked
        };

        let moved = items.len();
        if let Some(list) = self.get_mut(target) {
            list.add_items(items, None);
            if playing_moved {
                list.set_current(playing_id);
            }
        }

        if !copy {
            let active = self.active;
            if let Some(list) = self.playlists.iter_mut().find(|list| list.id == active) {
                list.remove(&ids);
            }
            // Записи очереди остаются в силе, но указывают уже на другую вкладку.
            for entry in self.queue.iter_mut() {
                if entry.playlist_id == active && ids.contains(&entry.item_id) {
                    entry.playlist_id = target;
                }
            }
            if playing_moved {
                self.playing = target;
            }
        }

        moved
    }

    pub fn enqueue(&mut self, playlist_id: u64, item_ids: &[u64]) {
        for item_id in item_ids {
            let entry = QueueEntry {
                playlist_id,
                item_id: *item_id,
            };
            if !self.queue.contains(&entry) {
                self.queue.push_back(entry);
            }
        }
    }

    pub fn queue_entries(&self) -> impl Iterator<Item = &QueueEntry> {
        self.queue.iter()
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    pub fn remove_from_queue(&mut self, item_id: u64) {
        self.queue.retain(|entry| entry.item_id != item_id);
    }

    /// Что играть следующим. Очередь всегда важнее обычного порядка.
    pub fn next_playback(
        &mut self,
        repeat: RepeatMode,
        shuffle: bool,
        manual: bool,
        consume_queue: bool,
    ) -> Option<(u64, u64, PathBuf)> {
        if let Some(entry) = self.queue.front().copied() {
            let found = self
                .get(entry.playlist_id)
                .and_then(|list| list.item(entry.item_id))
                .map(|item| item.path.clone());
            match found {
                Some(path) => {
                    if consume_queue {
                        self.queue.pop_front();
                    }
                    return Some((entry.playlist_id, entry.item_id, path));
                }
                // Строка исчезла — выбрасываем запись и пробуем дальше.
                None => {
                    self.queue.pop_front();
                    return self.next_playback(repeat, shuffle, manual, consume_queue);
                }
            }
        }

        let playing = self.playing;
        let list = self.get_mut(playing)?;
        let id = list.next_playback_id(repeat, shuffle, manual)?;
        let path = list.item(id)?.path.clone();
        Some((playing, id, path))
    }

    pub fn prev_playback(&mut self, shuffle: bool) -> Option<(u64, u64, PathBuf)> {
        let playing = self.playing;
        let list = self.get_mut(playing)?;
        let id = list.prev_playback_id(shuffle)?;
        let path = list.item(id)?.path.clone();
        Some((playing, id, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::PlaybackMode;

    fn with_tracks(count: usize) -> Collection {
        let mut collection = Collection::default();
        let paths = (1..=count)
            .map(|index| PathBuf::from(format!("/music/{index:02}.flac")))
            .collect();
        let items = collection.build_items(paths);
        collection.active_mut().add_items(items, None);
        collection
    }

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|name| PathBuf::from(format!("/music/{name}.flac"))).collect()
    }

    /// Новый круг перемешивания начинается с выбранного трека, и до конца
    /// круга звучат все остальные. Без этого выбранный оказывался посреди
    /// круга, собранного раньше, а треки «до» него не звучали вовсе.
    #[test]
    fn restarted_shuffle_starts_with_the_chosen_track() {
        for _ in 0..20 {
            let mut collection = with_tracks(6);
            let list = collection.active_mut();
            // Круг собран, когда текущего ещё не было, — как у свежей вкладки.
            let _ = list.next_playback_id(RepeatMode::Off, true, false);
            let chosen = list.items()[3].id;
            list.set_current(Some(chosen));
            list.restart_shuffle();

            let mut heard = vec![chosen];
            while let Some(next) = list.next_playback_id(RepeatMode::Off, true, false) {
                list.set_current(Some(next));
                heard.push(next);
                assert!(heard.len() <= 6, "круг не должен идти дальше шести треков");
            }
            heard.sort_unstable();
            heard.dedup();
            assert_eq!(heard.len(), 6, "за круг прозвучали все треки");
        }
    }

    /// Открытые из проводника файлы не попадают в собранный руками список и
    /// не меняют вкладку на экране.
    #[test]
    fn opened_files_get_their_own_tab() {
        let mut collection = with_tracks(3);
        let items = collection.build_items(paths(&["a", "b"]));
        let opened = collection.fill_opened(items);

        assert_ne!(opened, 1);
        assert_eq!(collection.active_id(), 1, "открытая вкладка не сменилась");
        assert_eq!(collection.active().items().len(), 3, "активный список не тронут");
        let tab = collection.get(opened).expect("вкладка есть");
        assert_eq!(tab.name, "Открытые файлы");
        assert_eq!(tab.items().len(), 2);
    }

    /// Вкладка одна и каждый раз собирается заново — это очередь «вот эти
    /// файлы», а не копилка всего, что когда-то открывали.
    #[test]
    fn opened_tab_is_reused_and_replaced() {
        let mut collection = with_tracks(1);
        let first = collection.build_items(paths(&["a", "b", "c"]));
        let opened = collection.fill_opened(first);
        let queued = collection.get(opened).map(|tab| tab.items()[1].id).unwrap_or_default();
        collection.enqueue(opened, &[queued]);

        let second = collection.build_items(paths(&["d"]));
        assert_eq!(collection.fill_opened(second), opened, "вкладка та же");
        assert_eq!(collection.tabs().len(), 2, "новых вкладок не появилось");

        let names: Vec<String> = collection
            .get(opened)
            .map(|tab| tab.items().iter().map(|item| item.title.clone()).collect())
            .unwrap_or_default();
        assert_eq!(names, vec!["d".to_owned()]);
        assert_eq!(collection.queue_entries().count(), 0, "очередь на старые строки снята");
    }

    /// Переименованную вкладку узнаём по номеру, закрытую — создаём заново.
    #[test]
    fn opened_tab_survives_rename_and_close() {
        let mut collection = with_tracks(1);
        let items = collection.build_items(paths(&["a"]));
        let opened = collection.fill_opened(items);
        collection.rename(opened, "Моё".to_owned());

        let again = collection.build_items(paths(&["b"]));
        assert_eq!(collection.fill_opened(again), opened);

        collection.close(opened);
        let fresh = collection.build_items(paths(&["c"]));
        let reopened = collection.fill_opened(fresh);
        assert_ne!(reopened, opened);
        assert_eq!(collection.tabs().len(), 2);
    }

    /// Файл убрали в корзину — строки должны исчезнуть из всех вкладок и из
    /// очереди, а не остаться указывать в пустоту.
    #[test]
    fn drop_paths_clears_every_tab() {
        let mut collection = with_tracks(3);
        let gone = collection.active().items()[1].path.clone();
        let queued = collection.active().items()[1].id;
        collection.enqueue(1, &[queued]);
        let second = collection.create(Some("Копия".to_owned()));
        collection.set_active(1);
        collection.transfer(&[1], second, true);

        let dropped = collection.drop_paths(std::slice::from_ref(&gone));
        assert_eq!(dropped, 2, "строка была в двух вкладках");
        assert!(collection.all_items().all(|item| item.path != gone));
        assert_eq!(collection.queue_entries().count(), 0);
    }

    /// Переименование меняет путь у строки, а не пересоздаёт её.
    #[test]
    fn replace_paths_keeps_the_row() {
        let mut collection = with_tracks(2);
        let before = collection.active().items()[0].clone();
        let after = PathBuf::from("/music/Хаски - Панелька.flac");

        let touched = collection.replace_paths(&[(before.path.clone(), after.clone())]);
        assert_eq!(touched, 1);

        let item = &collection.active().items()[0];
        assert_eq!(item.id, before.id, "номер строки тот же");
        assert_eq!(item.path, after);
        // Название бралось из имени файла, значит должно было обновиться.
        assert_eq!(item.title, "Хаски - Панелька");
    }

    /// Перенос: строки уезжают целиком, вместе с прочитанными тегами.
    #[test]
    fn transfer_moves_rows_to_another_tab() {
        let mut collection = with_tracks(4);
        let ids: Vec<u64> = collection.active().items().iter().map(|item| item.id).collect();
        let target = collection.create(Some("Вторая".to_owned()));
        // create делает новую вкладку активной — возвращаемся к исходной.
        collection.set_active(1);
        assert_eq!(collection.active().items().len(), 4, "проверяем исходную вкладку");

        let moved = collection.transfer(&[1, 2], target, false);
        assert_eq!(moved, 2);
        assert_eq!(collection.active().items().len(), 2);
        let left: Vec<u64> = collection.active().items().iter().map(|item| item.id).collect();
        assert_eq!(left, vec![ids[0], ids[3]]);

        let arrived: Vec<u64> = collection
            .get(target)
            .map(|list| list.items().iter().map(|item| item.id).collect())
            .unwrap_or_default();
        assert_eq!(arrived, vec![ids[1], ids[2]], "номера строк сохраняются");
    }

    /// Копия остаётся и в исходной вкладке, но с новыми номерами строк:
    /// номера общие на всю коллекцию, и очередь путала бы их.
    #[test]
    fn transfer_can_copy() {
        let mut collection = with_tracks(3);
        let ids: Vec<u64> = collection.active().items().iter().map(|item| item.id).collect();
        let target = collection.create(Some("Копии".to_owned()));
        collection.set_active(1);

        let copied = collection.transfer(&[0, 0, 1], target, true);
        assert_eq!(copied, 2, "повторы в выборе не удваивают строки");
        assert_eq!(collection.active().items().len(), 3, "исходная вкладка цела");

        let arrived = collection.get(target).map(|list| list.items().to_vec()).unwrap_or_default();
        assert_eq!(arrived.len(), 2);
        assert!(arrived.iter().all(|item| !ids.contains(&item.id)));
        assert_eq!(arrived[0].path, collection.active().items()[0].path);
    }

    /// Играющая строка уезжает вместе с отметкой: иначе музыка идёт из
    /// вкладки, где этой строки больше нет.
    #[test]
    fn transfer_takes_the_playing_row_with_it() {
        let mut collection = with_tracks(3);
        let playing = collection.active().items()[1].id;
        collection.active_mut().set_current(Some(playing));
        collection.enqueue(1, &[collection.active().items()[2].id]);
        let queued = collection.active().items()[2].id;
        let target = collection.create(Some("Туда".to_owned()));
        collection.set_active(1);

        assert_eq!(collection.transfer(&[1, 2], target, false), 2);
        assert_eq!(collection.playing_id(), target);
        assert_eq!(collection.get(target).and_then(|list| list.current_id()), Some(playing));
        assert_eq!(collection.active().current_id(), None);

        // Запись очереди осталась в силе, но указывает уже на другую вкладку.
        let entry = collection.queue_entries().next().copied();
        assert_eq!(entry.map(|entry| (entry.playlist_id, entry.item_id)), Some((target, queued)));
    }

    #[test]
    fn transfer_refuses_nowhere() {
        let mut collection = with_tracks(2);
        assert_eq!(collection.transfer(&[0], 1, false), 0, "в ту же вкладку");
        assert_eq!(collection.transfer(&[0], 777, false), 0, "в несуществующую");
        assert_eq!(collection.transfer(&[], 1, false), 0);
        assert_eq!(collection.active().items().len(), 2);
    }

    /// Без перемешивания «дальше» — это просто следующие строки, а с ним —
    /// порядок из `shuffle_order`, которого в самом списке не видно.
    #[test]
    fn upcoming_follows_playback_order() {
        let mut collection = with_tracks(5);
        let first = collection.active().items()[0].id;
        collection.active_mut().set_current(Some(first));

        let plain = collection
            .active_mut()
            .upcoming_ids(RepeatMode::Off, false, 10);
        let expected: Vec<u64> = collection.active().items()[1..]
            .iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(plain, expected);

        let shuffled = collection
            .active_mut()
            .upcoming_ids(RepeatMode::Off, true, 10);
        assert_eq!(shuffled.len(), 4, "текущий трек в список не попадает");
        assert!(!shuffled.contains(&first));
        let mut sorted = shuffled.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "повторов быть не должно");

        // С повтором списка очередь заворачивается: после последнего трека
        // снова придёт текущий, и показать его честнее, чем оборвать список.
        let looped = collection
            .active_mut()
            .upcoming_ids(RepeatMode::All, false, 10);
        assert_eq!(looped.len(), 5);
        assert_eq!(looped.last(), Some(&first));
    }

    /// Поиск идёт по всем словам сразу и не различает регистр.
    #[test]
    fn search_matches_every_word() {
        let mut collection = Collection::default();
        let items = collection.build_items(vec![
            PathBuf::from("/music/Кишлак - Ночь.flac"),
            PathBuf::from("/music/other/Night Train.mp3"),
        ]);
        collection.active_mut().add_items(items, None);

        let (total, rows) = collection.active().search("кишлак ночь", 0, 10);
        assert_eq!(total, 1);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].path.contains("Кишлак"));

        assert_eq!(collection.active().search("NIGHT", 0, 10).0, 1);
        assert_eq!(collection.active().search("кишлак train", 0, 10).0, 0);
        assert_eq!(collection.active().search("   ", 0, 10).0, 0);
    }

    /// Режим у каждой вкладки свой и переживает перезапуск: именно из-за
    /// этого он лежит в списке, а не только в настройках плеера.
    #[test]
    fn mode_is_per_tab_and_survives_saving() {
        let mut collection = with_tracks(2);
        let first = collection.active_id();
        let second = collection.create(Some("Второй".into()));

        assert_eq!(collection.get(first).and_then(|list| list.mode()), None);
        collection.get_mut(first).expect("вкладка").set_mode(PlaybackMode {
            repeat: RepeatMode::All,
            shuffle: true,
        });

        let json = serde_json::to_string(&collection).expect("сериализация");
        let restored: Collection = serde_json::from_str(&json).expect("разбор");

        assert_eq!(
            restored.get(first).and_then(|list| list.mode()),
            Some(PlaybackMode {
                repeat: RepeatMode::All,
                shuffle: true
            })
        );
        assert_eq!(
            restored.get(second).and_then(|list| list.mode()),
            None,
            "чужой режим вкладке не достаётся"
        );
    }

    #[test]
    fn tabs_are_independent_and_ids_are_global() {
        let mut collection = with_tracks(2);
        let first_tab = collection.active_id();
        let second_tab = collection.create(Some("Второй".into()));

        let items = collection.build_items(vec![PathBuf::from("/music/x.flac")]);
        let new_id = items[0].id;
        collection.active_mut().add_items(items, None);

        assert_eq!(collection.active_id(), second_tab);
        assert_eq!(collection.get(second_tab).expect("вкладка").items().len(), 1);
        assert_eq!(collection.get(first_tab).expect("вкладка").items().len(), 2);
        assert_eq!(new_id, 3, "идентификаторы строк общие на все вкладки");
    }

    #[test]
    fn queue_wins_over_normal_order() {
        let mut collection = with_tracks(3);
        let playlist = collection.active_id();
        let ids: Vec<u64> = collection
            .active()
            .items()
            .iter()
            .map(|item| item.id)
            .collect();

        collection.active_mut().set_current(Some(ids[0]));
        collection.enqueue(playlist, &[ids[2]]);

        let next = collection
            .next_playback(RepeatMode::Off, false, false, true)
            .expect("очередь");
        assert_eq!(next.1, ids[2], "первым должен идти трек из очереди");

        // Очередь опустела — дальше обычный порядок.
        let next = collection
            .next_playback(RepeatMode::Off, false, false, true)
            .expect("следующий");
        assert_eq!(next.1, ids[1]);
    }

    #[test]
    fn tabs_can_be_reordered() {
        let mut collection = Collection::default();
        let second = collection.create(Some("Два".into()));
        let third = collection.create(Some("Три".into()));
        collection.move_tab(third, Some(1));
        let order: Vec<u64> = collection.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, [third, 1, second]);
        collection.move_tab(third, None);
        let order: Vec<u64> = collection.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, [1, second, third]);
        // Перед самой собой и перед несуществующей — без сюрпризов.
        collection.move_tab(second, Some(second));
        collection.move_tab(1, Some(999));
        let order: Vec<u64> = collection.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, [second, third, 1]);
    }

    #[test]
    fn closing_a_tab_keeps_at_least_one() {
        let mut collection = with_tracks(1);
        let first = collection.active_id();
        collection.close(first);
        assert_eq!(collection.tabs().len(), 1, "последнюю вкладку не закрываем");

        let second = collection.create(None);
        collection.close(second);
        assert_eq!(collection.active_id(), first);
    }

    #[test]
    fn playing_tab_survives_switching() {
        let mut collection = with_tracks(2);
        let first = collection.active_id();
        let second = collection.create(None);

        collection.set_playing(first);
        collection.set_active(second);
        assert_eq!(collection.playing_id(), first, "вкладку переключили, играет прежняя");
    }
}
