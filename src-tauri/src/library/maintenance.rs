//! Проверка коллекции: дубликаты и пропавшие файлы.
//!
//! Обе проверки только **показывают** найденное. Решает пользователь: плеер
//! сам ничего не удаляет и не выбирает, какая из копий лишняя — по тегам это
//! не всегда видно (концертная запись и студийная называются одинаково).

use rusqlite::Connection;

use super::models::{LibraryTrack, TRACK_COLUMNS};
use crate::error::Result;

/// Группа похожих треков. Ключ нужен фронту для `{#each}` и для заголовка.
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DuplicateGroup.ts")]
pub struct DuplicateGroup {
    pub key: String,
    pub tracks: Vec<LibraryTrack>,
}

/// Насколько могут разойтись длительности одного и того же трека: у mp3 и
/// flac одной записи разница в доли секунды.
const DURATION_TOLERANCE_MS: u64 = 3_000;

/// Треки, у которых совпали исполнитель с названием (а без тегов — имя файла)
/// **и** длительность.
///
/// Сравнение и группировка в Rust, а не в SQL: без ICU у SQLite `lower()`
/// понимает только латиницу, а половина коллекции по-русски.
///
/// Одного названия мало: в живой коллекции теги врут, и три разные песни
/// «Where Are You Now» с одинаково испорченным исполнителем склеивались в
/// одну группу. Длительность их разводит, а настоящие копии (тот же трек в
/// mp3 и flac) оставляет вместе.
pub fn duplicates(connection: &Connection, limit: usize) -> Result<Vec<DuplicateGroup>> {
    let sql = format!("SELECT {TRACK_COLUMNS} FROM tracks");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], LibraryTrack::from_row)?;

    let mut groups: std::collections::HashMap<String, Vec<LibraryTrack>> =
        std::collections::HashMap::new();
    for track in rows {
        let track = track?;
        groups.entry(duplicate_key(&track)).or_default().push(track);
    }

    let mut found: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, tracks)| tracks.len() > 1)
        .flat_map(|(key, tracks)| split_by_duration(&key, tracks))
        .collect();

    // Сначала самые «толстые» группы: с них и начинают разбираться.
    found.sort_by(|left, right| {
        right
            .tracks
            .len()
            .cmp(&left.tracks.len())
            .then_with(|| left.key.cmp(&right.key))
    });
    found.truncate(limit);
    Ok(found)
}

/// Делит группу на настоящие копии: одна длительность — одна группа.
/// Длительность попадает и в ключ: он должен быть уникальным (на дубле ключа
/// `{#each}` во фронте роняет список), а заодно её полезно видеть.
fn split_by_duration(key: &str, mut tracks: Vec<LibraryTrack>) -> Vec<DuplicateGroup> {
    tracks.sort_by(|left, right| {
        left.duration_ms
            .cmp(&right.duration_ms)
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut groups: Vec<Vec<LibraryTrack>> = Vec::new();
    for track in tracks {
        let fits = groups.last().and_then(|group| group.first()).is_some_and(|first| {
            match (first.duration_ms, track.duration_ms) {
                (Some(left), Some(right)) => right.abs_diff(left) <= DURATION_TOLERANCE_MS,
                // Неизвестная длительность ни к кому не примыкает: у сломанного
                // файла её нет, и складывать такие вместе смысла нет.
                _ => false,
            }
        });
        match (fits, groups.last_mut()) {
            (true, Some(group)) => group.push(track),
            _ => groups.push(vec![track]),
        }
    }

    groups
        .into_iter()
        .filter(|group| group.len() > 1)
        .map(|mut group| {
            group.sort_by(|left, right| left.path.cmp(&right.path));
            let length = group
                .first()
                .and_then(|track| track.duration_ms)
                .map(|ms| format!(" · {}:{:02}", ms / 60_000, ms / 1000 % 60))
                .unwrap_or_default();
            DuplicateGroup {
                key: format!("{key}{length}"),
                tracks: group,
            }
        })
        .collect()
}

/// Ключ сравнения: исполнитель и название без регистра и пунктуации, а если
/// тегов нет — имя файла. Иначе безымянные `track01.mp3` из разных папок
/// склеились бы в одну кучу.
fn duplicate_key(track: &LibraryTrack) -> String {
    let artist = normalize(track.artist.as_deref().unwrap_or_default());
    let title = normalize(&track.title);
    if !artist.is_empty() && !title.is_empty() {
        return format!("{artist} — {title}");
    }
    if !title.is_empty() {
        return title;
    }
    normalize(file_stem(&track.path))
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn file_stem(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path)
}

/// Треки, которых больше нет на диске.
///
/// Файловая система опрашивается по одному пути — на десятках тысяч строк это
/// секунды, поэтому команда блокирующая и с ограничением.
pub fn missing(connection: &Connection, limit: usize) -> Result<Vec<LibraryTrack>> {
    let sql = format!("SELECT {TRACK_COLUMNS} FROM tracks ORDER BY path");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], LibraryTrack::from_row)?;

    let mut gone = Vec::new();
    for track in rows {
        let track = track?;
        if !std::path::Path::new(&track.path).is_file() {
            gone.push(track);
            if gone.len() >= limit {
                break;
            }
        }
    }
    Ok(gone)
}

/// Убирает строки из библиотеки. Файлы не трогает: их либо уже нет, либо с
/// ними отдельно разбираются через корзину.
pub fn forget(connection: &Connection, paths: &[String]) -> Result<u32> {
    let mut statement = connection.prepare_cached("DELETE FROM tracks WHERE path = ?1")?;
    let mut removed = 0;
    for path in paths {
        removed += statement.execute([path])? as u32;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let connection = Connection::open_in_memory().expect("база");
        connection
            .execute_batch(
                "CREATE TABLE tracks (
                    id INTEGER PRIMARY KEY, path TEXT, title TEXT, artist TEXT,
                    album TEXT, year INTEGER, track_no INTEGER, duration_ms INTEGER,
                    play_count INTEGER NOT NULL DEFAULT 0,
                    rating INTEGER NOT NULL DEFAULT 0,
                    last_played INTEGER
                );",
            )
            .expect("схема");
        connection
    }

    fn add(connection: &Connection, path: &str, artist: Option<&str>, title: Option<&str>) {
        add_long(connection, path, artist, title, Some(200_000));
    }

    fn add_long(
        connection: &Connection,
        path: &str,
        artist: Option<&str>,
        title: Option<&str>,
        duration_ms: Option<u64>,
    ) {
        // В схеме название NOT NULL: сканер кладёт туда имя файла, если тега нет.
        let title = title.unwrap_or("без названия");
        connection
            .execute(
                "INSERT INTO tracks (path, title, artist, duration_ms) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![path, title, artist, duration_ms.map(|ms| ms as i64)],
            )
            .expect("вставка трека");
    }

    #[test]
    fn duplicates_ignore_case_and_punctuation() {
        let connection = db();
        add(&connection, "/a/1.mp3", Some("Хаски"), Some("Панелька"));
        add(&connection, "/b/2.mp3", Some("хаски"), Some("панелька!"));
        add(&connection, "/c/3.mp3", Some("Хаски"), Some("Ай"));

        let found = duplicates(&connection, 10).expect("поиск");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tracks.len(), 2);
        assert_eq!(found[0].key, "хаски — панелька · 3:20");
        // Порядок внутри группы — по пути, чтобы список не прыгал.
        assert_eq!(found[0].tracks[0].path, "/a/1.mp3");
    }

    /// Без тегов сканер кладёт в название имя файла — по нему и сравниваем,
    /// иначе все «track01» из разных папок склеились бы в одну кучу.
    #[test]
    fn untagged_files_group_by_name() {
        let connection = db();
        add(&connection, "/a/track01.mp3", None, Some("track01"));
        add(&connection, "/b/Track01.mp3", None, Some("Track01"));
        add(&connection, "/c/track02.mp3", None, Some("track02"));
        // Пустое название встречается: тогда в дело идёт имя файла.
        add(&connection, "/d/track01.mp3", None, Some(""));

        let found = duplicates(&connection, 10).expect("поиск");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].key.starts_with("track01"), "{}", found[0].key);
        assert_eq!(found[0].tracks.len(), 3, "пустое название попало в ту же группу");
    }

    /// Одинаковое название при разной длительности — это разные песни, и в
    /// живой коллекции так бывает из-за врущих тегов.
    #[test]
    fn different_lengths_are_not_duplicates() {
        let connection = db();
        add_long(&connection, "/a/1.flac", Some("A"), Some("Где ты"), Some(200_000));
        add_long(&connection, "/b/1.mp3", Some("A"), Some("Где ты"), Some(200_400));
        add_long(&connection, "/c/1.flac", Some("A"), Some("Где ты"), Some(260_000));

        let found = duplicates(&connection, 10).expect("поиск");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].tracks.len(), 2, "вместе только совпавшие по времени");
        assert!(found[0].key.ends_with("3:20"), "{}", found[0].key);
    }

    /// Группы отдаются от самых больших: с них и начинают разбираться.
    #[test]
    fn biggest_groups_come_first() {
        let connection = db();
        for index in 0..3 {
            add(&connection, &format!("/a/{index}.mp3"), Some("A"), Some("Три"));
        }
        add(&connection, "/b/1.mp3", Some("B"), Some("Два"));
        add(&connection, "/b/2.mp3", Some("B"), Some("Два"));

        let found = duplicates(&connection, 10).expect("поиск");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].tracks.len(), 3);
        assert_eq!(found[1].tracks.len(), 2);
    }

    #[test]
    fn missing_finds_only_gone_files() {
        let path = crate::audio::test_wav::temp_path("maintenance");
        crate::audio::test_wav::sine(&path, 44_100, 2, 4_410, 0.5);
        let alive = path.to_string_lossy().into_owned();

        let connection = db();
        add(&connection, &alive, Some("Есть"), Some("Есть"));
        add(&connection, "/такого/нет.mp3", Some("Нет"), Some("Нет"));

        let gone = missing(&connection, 100).expect("поиск");
        let _ = std::fs::remove_file(&path);
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].path, "/такого/нет.mp3");
    }

    #[test]
    fn forget_removes_rows_only() {
        let connection = db();
        add(&connection, "/a/1.mp3", Some("A"), Some("B"));
        add(&connection, "/a/2.mp3", Some("A"), Some("C"));

        let removed = forget(&connection, &["/a/1.mp3".to_owned()]).expect("удаление");
        assert_eq!(removed, 1);
        let left: u32 = connection
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .expect("счёт");
        assert_eq!(left, 1);
    }
}
