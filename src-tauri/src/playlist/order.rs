//! Сортировка, перетаскивание строк и порядок воспроизведения.

use std::time::{SystemTime, UNIX_EPOCH};

use super::{Playlist, PlaylistSort, SortKey};
use crate::settings::RepeatMode;

impl Playlist {
    pub fn sort_by(&mut self, key: SortKey, ascending: bool) {
        // sort_by в Rust стабильная, поэтому прошлый порядок работает
        // вторичным ключом — при равных значениях строки не скачут.
        match key {
            SortKey::Manual => {}
            SortKey::Title => self
                .items
                .sort_by(|a, b| compare_text(&a.title, &b.title)),
            SortKey::Artist => self.items.sort_by(|a, b| {
                compare_opt_text(a.artist.as_deref(), b.artist.as_deref())
            }),
            SortKey::Album => self
                .items
                .sort_by(|a, b| compare_opt_text(a.album.as_deref(), b.album.as_deref())),
            SortKey::Duration => self.items.sort_by(|a, b| {
                a.duration_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.duration_ms.unwrap_or(u64::MAX))
            }),
            SortKey::TrackNo => self.items.sort_by(|a, b| {
                a.track_no
                    .unwrap_or(u32::MAX)
                    .cmp(&b.track_no.unwrap_or(u32::MAX))
            }),
            SortKey::Path => self.items.sort_by(|a, b| a.path.cmp(&b.path)),
            SortKey::Year => self
                .items
                .sort_by_key(|item| item.year.unwrap_or(u32::MAX)),
            SortKey::Genre => self
                .items
                .sort_by(|a, b| compare_opt_text(a.genre.as_deref(), b.genre.as_deref())),
            SortKey::Bitrate => self
                .items
                .sort_by_key(|item| item.bitrate_kbps.unwrap_or(u32::MAX)),
        }

        if !ascending {
            self.items.reverse();
        }
        self.sort = PlaylistSort { key, ascending };
        self.bump_version();
        self.shuffle_order.clear();
    }

    /// Перетаскивание строк: выделенные едут на позицию `to_index`
    /// в том порядке, в каком они были в списке.
    pub fn move_items(&mut self, ids: &[u64], to_index: usize) {
        if ids.is_empty() {
            return;
        }

        let mut moving = Vec::with_capacity(ids.len());
        let mut rest = Vec::with_capacity(self.items.len());
        // Сколько перемещаемых строк было выше точки вставки — на столько
        // она уезжает вверх после их изъятия.
        let mut removed_before = 0usize;

        for (index, item) in std::mem::take(&mut self.items).into_iter().enumerate() {
            if ids.contains(&item.id) {
                if index < to_index {
                    removed_before += 1;
                }
                moving.push(item);
            } else {
                rest.push(item);
            }
        }

        let insert_at = to_index.saturating_sub(removed_before).min(rest.len());
        rest.splice(insert_at..insert_at, moving);
        self.items = rest;
        self.sort = PlaylistSort::default();
        self.bump_version();
        self.shuffle_order.clear();
    }

    /// Что играть следующим. `manual` — пользователь нажал «вперёд»:
    /// в этом случае повтор одного трека игнорируется, иначе кнопка
    /// выглядела бы сломанной.
    pub fn next_playback_id(&mut self, repeat: RepeatMode, shuffle: bool, manual: bool) -> Option<u64> {
        if self.items.is_empty() {
            return None;
        }
        if repeat == RepeatMode::Track && !manual {
            return self.current.or_else(|| self.items.first().map(|item| item.id));
        }

        let Some(current) = self.current_index() else {
            return self.first_playback_id(shuffle);
        };

        if shuffle {
            self.ensure_shuffle_order();
            let position = self.shuffle_position(current)?;
            if let Some(&next) = self.shuffle_order.get(position + 1) {
                return self.items.get(next).map(|item| item.id);
            }
            if repeat == RepeatMode::All || manual {
                self.shuffle_order.clear();
                self.ensure_shuffle_order();
                let first = *self.shuffle_order.first()?;
                return self.items.get(first).map(|item| item.id);
            }
            return None;
        }

        if let Some(item) = self.items.get(current + 1) {
            return Some(item.id);
        }
        if repeat == RepeatMode::All || manual {
            return self.items.first().map(|item| item.id);
        }
        None
    }

    /// Что заиграет дальше, в порядке воспроизведения и не меняя состояния.
    ///
    /// При перемешивании порядок виден только здесь: в самом списке строки
    /// стоят как стояли, а играют они по `shuffle_order`.
    pub fn upcoming_ids(&mut self, repeat: RepeatMode, shuffle: bool, limit: usize) -> Vec<u64> {
        if self.items.is_empty() || limit == 0 {
            return Vec::new();
        }
        // Повтор одного трека: дальше будет он же, показывать очередь незачем.
        if repeat == RepeatMode::Track {
            return self.current.into_iter().collect();
        }

        let order: Vec<usize> = if shuffle {
            self.ensure_shuffle_order();
            self.shuffle_order.clone()
        } else {
            (0..self.items.len()).collect()
        };

        let start = self
            .current_index()
            .and_then(|current| order.iter().position(|&index| index == current))
            .map(|position| position + 1)
            .unwrap_or(0);

        let total = order.len();
        let wrap = repeat == RepeatMode::All;
        let mut ids = Vec::with_capacity(limit.min(total));
        for step in 0..total {
            let mut position = start + step;
            if position >= total {
                if !wrap {
                    break;
                }
                position -= total;
            }
            let Some(&index) = order.get(position) else {
                break;
            };
            if let Some(item) = self.items.get(index) {
                ids.push(item.id);
            }
            if ids.len() >= limit {
                break;
            }
        }
        ids
    }

    pub fn prev_playback_id(&mut self, shuffle: bool) -> Option<u64> {
        if self.items.is_empty() {
            return None;
        }
        let Some(current) = self.current_index() else {
            return self.first_playback_id(shuffle);
        };

        if shuffle {
            self.ensure_shuffle_order();
            let position = self.shuffle_position(current)?;
            let previous = position.checked_sub(1)?;
            let index = *self.shuffle_order.get(previous)?;
            return self.items.get(index).map(|item| item.id);
        }

        let index = current.checked_sub(1).unwrap_or(self.items.len() - 1);
        self.items.get(index).map(|item| item.id)
    }

    pub fn first_playback_id(&mut self, shuffle: bool) -> Option<u64> {
        if shuffle {
            self.ensure_shuffle_order();
            let index = *self.shuffle_order.first()?;
            return self.items.get(index).map(|item| item.id);
        }
        self.items.first().map(|item| item.id)
    }

    fn shuffle_position(&self, index: usize) -> Option<usize> {
        self.shuffle_order.iter().position(|&value| value == index)
    }

    /// Перемешанный порядок строится один раз на список и начинается с
    /// текущего трека — иначе первое же переключение уводило бы с него.
    fn ensure_shuffle_order(&mut self) {
        if self.shuffle_order.len() == self.items.len() {
            return;
        }
        self.shuffle_order = (0..self.items.len()).collect();

        let mut rng = Xorshift::seeded();
        let len = self.shuffle_order.len();
        for i in (1..len).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            self.shuffle_order.swap(i, j);
        }

        if let Some(current) = self.current_index()
            && let Some(position) = self.shuffle_position(current)
        {
            self.shuffle_order.swap(0, position);
        }
    }
}

fn compare_text(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn compare_opt_text(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        // Пустые значения всегда внизу, независимо от направления сортировки
        // это было бы приятнее, но тогда reverse() перестанет быть честным.
        (Some(a), Some(b)) => compare_text(a, b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Крошечный xorshift: тянуть `rand` ради одного перемешивания незачем.
struct Xorshift(u64);

impl Xorshift {
    fn seeded() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::playlist::Playlist;

    fn playlist(count: usize) -> Playlist {
        let mut playlist = Playlist::default();
        let items = (1..=count)
            .map(|index| {
                crate::playlist::PlaylistItem::from_path(
                    index as u64,
                    PathBuf::from(format!("/music/{index:02}.flac")),
                )
            })
            .collect();
        playlist.add_items(items, None);
        playlist
    }

    fn titles(playlist: &Playlist) -> Vec<String> {
        playlist
            .items()
            .iter()
            .map(|item| item.title.clone())
            .collect()
    }

    #[test]
    fn moves_selection_below_and_above() {
        let mut list = playlist(5);
        let third = list.at(2).map(|item| item.id).expect("третья строка");

        list.move_items(&[third], 0);
        assert_eq!(titles(&list), ["03", "01", "02", "04", "05"]);

        // Теперь вниз: точка вставки считается по списку ДО изъятия строк.
        let first = list.at(0).map(|item| item.id).expect("первая строка");
        list.move_items(&[first], 3);
        assert_eq!(titles(&list), ["01", "02", "03", "04", "05"]);
    }

    #[test]
    fn sorts_and_reverses() {
        let mut list = playlist(3);
        list.sort_by(SortKey::Title, false);
        assert_eq!(titles(&list), ["03", "02", "01"]);
        list.sort_by(SortKey::Title, true);
        assert_eq!(titles(&list), ["01", "02", "03"]);
    }

    #[test]
    fn walks_forward_and_stops_at_the_end() {
        let mut list = playlist(3);
        let first = list.at(0).map(|item| item.id).expect("первая строка");
        list.set_current(Some(first));

        let second = list.next_playback_id(RepeatMode::Off, false, false);
        assert_eq!(second, list.at(1).map(|item| item.id));

        let last = list.at(2).map(|item| item.id).expect("последняя строка");
        list.set_current(Some(last));
        assert_eq!(list.next_playback_id(RepeatMode::Off, false, false), None);
        assert_eq!(
            list.next_playback_id(RepeatMode::All, false, false),
            list.at(0).map(|item| item.id)
        );
    }

    #[test]
    fn repeat_track_only_applies_to_auto_advance() {
        let mut list = playlist(3);
        let first = list.at(0).map(|item| item.id).expect("первая строка");
        list.set_current(Some(first));

        assert_eq!(
            list.next_playback_id(RepeatMode::Track, false, false),
            Some(first),
            "в конце трека повтор одного должен вернуть тот же трек"
        );
        assert_eq!(
            list.next_playback_id(RepeatMode::Track, false, true),
            list.at(1).map(|item| item.id),
            "кнопка «вперёд» повтор одного игнорирует"
        );
    }

    #[test]
    fn previous_wraps_to_the_end() {
        let mut list = playlist(3);
        let first = list.at(0).map(|item| item.id).expect("первая строка");
        list.set_current(Some(first));
        assert_eq!(
            list.prev_playback_id(false),
            list.at(2).map(|item| item.id)
        );
    }

    #[test]
    fn shuffle_order_is_a_permutation_starting_at_current() {
        let mut list = playlist(50);
        let current = list.at(17).map(|item| item.id).expect("строка 17");
        list.set_current(Some(current));

        list.ensure_shuffle_order();
        let mut sorted = list.shuffle_order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>(), "каждая строка ровно раз");
        assert_eq!(
            list.shuffle_order.first(),
            Some(&17),
            "перемешанный порядок начинается с текущего трека"
        );

        // Проходим весь список: повторов быть не должно.
        let mut seen = vec![current];
        while let Some(next) = list.next_playback_id(RepeatMode::Off, true, false) {
            assert!(!seen.contains(&next), "трек повторился внутри одного круга");
            seen.push(next);
            list.set_current(Some(next));
        }
        assert_eq!(seen.len(), 50);
    }
}
