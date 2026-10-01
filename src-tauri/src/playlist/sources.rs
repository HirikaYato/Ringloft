//! Источники списка: папки, из которых он собран. По ним список обновляется —
//! новые файлы дописываются в конец, пропавшие с диска уходят.
//!
//! Тонкости:
//! - папка, которой сейчас нет (диск не подключён), **не** считается пустой:
//!   иначе отключённый диск вычищал бы список целиком;
//! - убранное руками не возвращается — для этого `excluded` у списка;
//! - строки вне источников (добавленные файлами, радио, CUE из импорта) не
//!   трогаем вовсе.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use super::{Playlist, PlaylistStore};

/// Итог обновления по источникам.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SourcesRefresh.ts")]
pub struct SourcesRefresh {
    pub added: u32,
    pub removed: u32,
    /// Папки, до которых не достали: их строки оставлены как были.
    pub unreachable: Vec<String>,
}

impl SourcesRefresh {
    pub fn merge(&mut self, other: SourcesRefresh) {
        self.added += other.added;
        self.removed += other.removed;
        self.unreachable.extend(other.unreachable);
    }
}

/// Источники одного списка — для диалога.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "PlaylistSources.ts")]
pub struct PlaylistSources {
    pub sources: Vec<String>,
    /// Сколько файлов из этих папок убрано руками и не вернётся при обновлении.
    pub excluded: u32,
}

impl PlaylistStore {
    pub fn sources_of(&self, playlist_id: u64) -> PlaylistSources {
        let collection = self.inner.read();
        let list = collection.get(playlist_id);
        PlaylistSources {
            sources: list
                .map(|list| list.sources().iter().map(|dir| dir.display().to_string()).collect())
                .unwrap_or_default(),
            excluded: list.map_or(0, |list| list.excluded_count() as u32),
        }
    }

    pub fn set_sources(&self, playlist_id: u64, dirs: Vec<PathBuf>) {
        if let Some(list) = self.inner.write().get_mut(playlist_id) {
            list.set_sources(dirs);
        }
        self.changed();
    }

    pub fn restore_excluded(&self, playlist_id: u64) {
        if let Some(list) = self.inner.write().get_mut(playlist_id) {
            list.clear_excluded();
        }
        self.changed();
    }

    /// Сверяет список с его папками. Обход диска идёт без блокировки: на
    /// большой коллекции это секунды, а список в это время должен жить.
    pub fn refresh_sources(&self, playlist_id: u64) -> SourcesRefresh {
        let sources = match self.inner.read().get(playlist_id) {
            Some(list) if !list.sources().is_empty() => list.sources().to_vec(),
            _ => return SourcesRefresh::default(),
        };
        let found = scan(&sources);
        let mut report = SourcesRefresh {
            unreachable: found
                .unreachable
                .iter()
                .map(|dir| dir.display().to_string())
                .collect(),
            ..SourcesRefresh::default()
        };

        let new_ids: Vec<u64> = {
            let mut collection = self.inner.write();
            // Пока обходили диск, список могли поменять — сверяем с тем, что
            // в нём сейчас.
            let Some(plan) = collection.get(playlist_id).map(|list| list.reconcile(&found)) else {
                return report;
            };
            let items = collection.build_items(plan.add);
            collection.forget(playlist_id, &plan.remove);
            report.added = items.len() as u32;
            report.removed = plan.remove.len() as u32;
            let ids = items.iter().map(|item| item.id).collect();
            if let Some(list) = collection.get_mut(playlist_id) {
                list.add_items(items, None);
                list.prune_excluded(&found);
            }
            ids
        };

        for id in new_ids {
            let _ = self.meta_tx.send(id);
        }
        if report.added > 0 || report.removed > 0 {
            self.changed();
        }
        report
    }

    /// Все списки с источниками по очереди — так делает запуск плеера.
    pub fn refresh_all_sources(&self) -> SourcesRefresh {
        let ids: Vec<u64> = {
            let collection = self.inner.read();
            collection
                .tabs()
                .iter()
                .filter(|tab| tab.sources > 0)
                .map(|tab| tab.id)
                .collect()
        };
        let mut total = SourcesRefresh::default();
        for id in ids {
            total.merge(self.refresh_sources(id));
        }
        total
    }
}

/// Что нашлось на диске. Обход идёт без блокировки списка.
pub(crate) struct SourceScan {
    pub reachable: Vec<PathBuf>,
    pub unreachable: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
}

pub(crate) fn scan(sources: &[PathBuf]) -> SourceScan {
    let mut found = SourceScan {
        reachable: Vec::new(),
        unreachable: Vec::new(),
        files: Vec::new(),
    };
    for dir in sources {
        if dir.is_dir() {
            super::store::collect_dir(dir, &mut found.files);
            found.reachable.push(dir.clone());
        } else {
            found.unreachable.push(dir.clone());
        }
    }
    found
}

/// Что сделать со списком: какие файлы дописать и какие строки убрать.
pub(crate) struct Reconcile {
    pub add: Vec<PathBuf>,
    pub remove: HashSet<u64>,
}

impl Playlist {
    pub fn sources(&self) -> &[PathBuf] {
        &self.sources
    }

    pub(crate) fn is_under_source(&self, path: &Path) -> bool {
        self.sources.iter().any(|dir| path.starts_with(dir))
    }

    /// Запоминает папку. Вложенная в уже известную ничего не добавляет, а
    /// объемлющая заменяет вложенные — иначе файлы считались бы дважды.
    pub fn add_source(&mut self, dir: PathBuf) {
        if self.is_under_source(&dir) {
            return;
        }
        self.sources.retain(|known| !known.starts_with(&dir));
        self.sources.push(dir);
        self.bump_version();
    }

    /// Новый набор из диалога. Исключения вне оставшихся папок теряют смысл.
    pub fn set_sources(&mut self, dirs: Vec<PathBuf>) {
        self.sources.clear();
        for dir in dirs {
            self.add_source(dir);
        }
        let sources = self.sources.clone();
        self.excluded
            .retain(|path| sources.iter().any(|dir| path.starts_with(dir)));
        self.bump_version();
    }

    pub fn excluded_count(&self) -> usize {
        self.excluded.len()
    }

    /// Вернуть всё, что убирали руками: следующее обновление их допишет.
    pub fn clear_excluded(&mut self) {
        self.excluded.clear();
    }

    pub(crate) fn reconcile(&self, scan: &SourceScan) -> Reconcile {
        let on_disk: HashSet<&Path> = scan.files.iter().map(PathBuf::as_path).collect();
        let listed: HashSet<&Path> = self.items.iter().map(|item| item.path.as_path()).collect();

        let add = scan
            .files
            .iter()
            .filter(|path| !listed.contains(path.as_path()) && !self.excluded.contains(*path))
            .cloned()
            .collect();
        let remove = self
            .items
            .iter()
            .filter(|item| {
                scan.reachable.iter().any(|dir| item.path.starts_with(dir))
                    && !on_disk.contains(item.path.as_path())
            })
            .map(|item| item.id)
            .collect();
        Reconcile { add, remove }
    }

    /// Исключения для файлов, которых уже нет на диске, хранить незачем.
    pub(crate) fn prune_excluded(&mut self, scan: &SourceScan) {
        let on_disk: HashSet<&Path> = scan.files.iter().map(PathBuf::as_path).collect();
        self.excluded.retain(|path| {
            !scan.reachable.iter().any(|dir| path.starts_with(dir)) || on_disk.contains(path.as_path())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::PlaylistItem;

    fn list(paths: &[&str]) -> Playlist {
        let mut list = Playlist::new_named(1, "тест".to_owned());
        let items = paths
            .iter()
            .enumerate()
            .map(|(index, path)| PlaylistItem::from_path(index as u64 + 1, PathBuf::from(path)))
            .collect();
        list.add_items(items, None);
        list
    }

    fn scan_of(reachable: &[&str], files: &[&str]) -> SourceScan {
        SourceScan {
            reachable: reachable.iter().map(PathBuf::from).collect(),
            unreachable: Vec::new(),
            files: files.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn nested_sources_collapse_into_the_outer_one() {
        let mut list = list(&[]);
        list.add_source("/music/rock".into());
        list.add_source("/music/rock/live".into());
        assert_eq!(list.sources(), [PathBuf::from("/music/rock")]);
        list.add_source("/music".into());
        assert_eq!(list.sources(), [PathBuf::from("/music")]);
    }

    #[test]
    fn adds_new_files_and_drops_vanished_ones() {
        let mut list = list(&["/m/a.flac", "/m/b.flac", "/other/c.mp3"]);
        list.add_source("/m".into());
        let plan = list.reconcile(&scan_of(&["/m"], &["/m/a.flac", "/m/new.flac"]));
        assert_eq!(plan.add, [PathBuf::from("/m/new.flac")]);
        // b.flac пропал с диска, а c.mp3 вне источников — его не трогаем.
        assert_eq!(plan.remove, HashSet::from([2]));
    }

    #[test]
    fn unreachable_folder_keeps_its_rows() {
        let mut list = list(&["/disk/a.flac"]);
        list.add_source("/disk".into());
        let plan = list.reconcile(&scan_of(&[], &[]));
        assert!(plan.add.is_empty());
        assert!(plan.remove.is_empty());
    }

    #[test]
    fn removed_by_hand_stays_removed() {
        let mut list = list(&["/m/a.flac", "/m/b.flac"]);
        list.add_source("/m".into());
        list.remove(&[2]);
        let scan = scan_of(&["/m"], &["/m/a.flac", "/m/b.flac"]);
        assert!(list.reconcile(&scan).add.is_empty());

        // Добавили руками — исключение снято.
        list.add_items(vec![PlaylistItem::from_path(9, "/m/b.flac".into())], None);
        assert_eq!(list.excluded_count(), 0);
    }

    #[test]
    fn exclusions_of_deleted_files_are_pruned() {
        let mut list = list(&["/m/a.flac", "/m/b.flac"]);
        list.add_source("/m".into());
        list.remove(&[2]);
        list.prune_excluded(&scan_of(&["/m"], &["/m/a.flac"]));
        assert_eq!(list.excluded_count(), 0);
    }

    #[test]
    fn clearing_forgets_sources() {
        let mut list = list(&["/m/a.flac"]);
        list.add_source("/m".into());
        list.clear();
        assert!(list.sources().is_empty());
    }
}
