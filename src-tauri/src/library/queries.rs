//! Запросы к библиотеке: статистика и поиск.

use rusqlite::Connection;

use super::models::{LibraryStats, LibraryTrack, TRACK_COLUMNS_T};
use crate::error::Result;

pub fn stats(connection: &Connection) -> Result<LibraryStats> {
    let stats = connection.query_row(
        "SELECT COUNT(*),
                COUNT(DISTINCT album_key),
                COUNT(DISTINCT COALESCE(NULLIF(album_artist, ''), NULLIF(artist, ''))),
                COALESCE(SUM(duration_ms), 0),
                COALESCE(SUM(size_bytes), 0)
         FROM tracks",
        [],
        |row| {
            Ok(LibraryStats {
                tracks: row.get::<_, i64>(0)? as u32,
                albums: row.get::<_, i64>(1)? as u32,
                artists: row.get::<_, i64>(2)? as u32,
                total_duration_ms: row.get::<_, i64>(3)? as u64,
                total_size_bytes: row.get::<_, i64>(4)? as u64,
            })
        },
    )?;
    Ok(stats)
}

pub fn search(connection: &Connection, query: &str, limit: u32) -> Result<Vec<LibraryTrack>> {
    let Some(expression) = fts_expression(query) else {
        return Ok(Vec::new());
    };

    let sql = format!(
        "SELECT {TRACK_COLUMNS_T}
         FROM tracks_fts f
         JOIN tracks t ON t.id = f.rowid
         WHERE tracks_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2"
    );
    let mut statement = connection.prepare_cached(&sql)?;

    let rows = statement.query_map(
        rusqlite::params![expression, limit],
        LibraryTrack::from_row,
    )?;

    let mut tracks = Vec::new();
    for row in rows {
        tracks.push(row?);
    }
    Ok(tracks)
}

/// Превращает пользовательский ввод в безопасное выражение FTS5.
///
/// Всё, что ввёл человек, — это данные, а не синтаксис: каждое слово берём в
/// кавычки (внутренние удваиваем) и добавляем `*`, чтобы работал поиск по
/// началу слова. Иначе любой `AND`, `-` или кавычка ломали бы запрос.
fn fts_expression(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect();

    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Отмечает трек прослушанным. Путь может быть и не из библиотеки (играли
/// файл со стороны) — тогда обновлять нечего, и это не ошибка.
/// Пути всех треков: нужны пакетным задачам (тексты, громкость).
pub fn all_paths(connection: &Connection, limit: u32) -> Result<Vec<String>> {
    let mut statement =
        connection.prepare_cached("SELECT path FROM tracks ORDER BY folder, path LIMIT ?1")?;
    let rows = statement.query_map([limit], |row| row.get::<_, String>(0))?;

    let mut paths = Vec::new();
    for path in rows {
        paths.push(path?);
    }
    Ok(paths)
}

/// Оценка: 0 — снять, иначе 1..5.
pub fn set_rating(connection: &Connection, path: &str, rating: u32) -> Result<bool> {
    let changed = connection.execute(
        "UPDATE tracks SET rating = ?2 WHERE path = ?1",
        rusqlite::params![path, i64::from(rating.min(5))],
    )?;
    Ok(changed > 0)
}

/// Прослушивания и оценка для набора путей. Пачками: у SQLite предел на
/// число параметров в запросе.
pub fn track_marks(
    connection: &Connection,
    paths: &[&str],
) -> Result<std::collections::HashMap<String, (u32, u32)>> {
    let mut marks = std::collections::HashMap::new();
    for chunk in paths.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let mut statement = connection.prepare(&format!(
            "SELECT path, play_count, rating FROM tracks WHERE path IN ({placeholders})"
        ))?;
        let rows = statement.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?, row.get::<_, u32>(2)?))
        })?;
        for row in rows {
            let (path, plays, rating) = row?;
            marks.insert(path, (plays, rating));
        }
    }
    Ok(marks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_user_input() {
        assert_eq!(fts_expression("nine inch"), Some("\"nine\"* \"inch\"*".into()));
        assert_eq!(fts_expression("  "), None);
        // Спецсинтаксис FTS5 не должен просачиваться наружу.
        assert_eq!(fts_expression("AND -foo"), Some("\"AND\"* \"-foo\"*".into()));
        assert_eq!(fts_expression("a\"b"), Some("\"a\"\"b\"*".into()));
    }
}
