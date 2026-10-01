//! Итоги прослушивания по истории `plays` (схема v3).
//!
//! Строка в истории — засчитанное прослушивание (порог тот же, что у
//! скробблинга). Время прослушивания считается по длительности трека: сколько
//! из него дослушали на самом деле, плеер не знает, а для итогов этого и не
//! надо.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;

/// Что засчитали: название и исполнитель на момент прослушивания.
#[derive(Debug, Clone)]
pub struct PlayRecord {
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "StatsPeriod.ts")]
pub enum StatsPeriod {
    Week,
    Month,
    Year,
    All,
}

impl StatsPeriod {
    fn days(self) -> Option<i64> {
        match self {
            Self::Week => Some(7),
            Self::Month => Some(30),
            Self::Year => Some(365),
            Self::All => None,
        }
    }

    /// Неделя и месяц — по дням, год и всё время — по месяцам.
    fn bucket_format(self) -> &'static str {
        match self {
            Self::Week | Self::Month => "%Y-%m-%d",
            Self::Year | Self::All => "%Y-%m",
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "StatsEntry.ts")]
pub struct StatsEntry {
    pub name: String,
    /// Исполнитель — у треков и альбомов.
    pub detail: Option<String>,
    /// Путь — у треков: из итогов трек можно включить.
    pub path: Option<String>,
    pub plays: u32,
    #[ts(type = "number")]
    pub listened_ms: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "StatsBucket.ts")]
pub struct StatsBucket {
    /// `2026-10-01` или `2026-10`.
    pub label: String,
    pub plays: u32,
    #[ts(type = "number")]
    pub listened_ms: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ListeningStats.ts")]
pub struct ListeningStats {
    pub plays: u32,
    #[ts(type = "number")]
    pub listened_ms: u64,
    pub tracks: u32,
    pub artists: u32,
    pub top_artists: Vec<StatsEntry>,
    pub top_tracks: Vec<StatsEntry>,
    pub top_albums: Vec<StatsEntry>,
    pub buckets: Vec<StatsBucket>,
    /// Прослушивания по часам суток (местное время), 24 числа.
    pub by_hour: Vec<u32>,
}

const TOP: i64 = 10;

/// История и счётчик трека — одной транзакцией.
pub fn record(connection: &mut Connection, play: &PlayRecord, at_unix: u64) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO plays (played_at, path, title, artist, album, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            at_unix as i64,
            play.path,
            play.title,
            play.artist,
            play.album,
            play.duration_ms as i64
        ],
    )?;
    transaction.execute(
        "UPDATE tracks SET play_count = play_count + 1, last_played = ?2 WHERE path = ?1",
        rusqlite::params![play.path, at_unix as i64],
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn listening(connection: &Connection, period: StatsPeriod, now_unix: u64) -> Result<ListeningStats> {
    let from = period
        .days()
        .map_or(0, |days| now_unix as i64 - days * 24 * 3600);

    let (plays, listened_ms, tracks, artists) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(duration_ms), 0), COUNT(DISTINCT path),
                COUNT(DISTINCT NULLIF(artist, ''))
         FROM plays WHERE played_at >= ?1",
        [from],
        |row| Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)?, row.get::<_, u32>(2)?, row.get::<_, u32>(3)?)),
    )?;

    // Голые столбцы рядом с MAX() SQLite берёт из той же строки, что и
    // максимум, — так у трека подпись последнего прослушивания.
    let top_tracks = entries(
        connection,
        "SELECT title, artist, path, COUNT(*), COALESCE(SUM(duration_ms), 0), MAX(played_at)
         FROM plays WHERE played_at >= ?1
         GROUP BY path ORDER BY 4 DESC, 6 DESC LIMIT ?2",
        from,
        |row| {
            Ok(StatsEntry {
                name: row.get(0)?,
                detail: row.get(1)?,
                path: row.get(2)?,
                plays: row.get(3)?,
                listened_ms: row.get::<_, i64>(4)? as u64,
            })
        },
    )?;
    let top_artists = entries(
        connection,
        "SELECT artist, COUNT(*), COALESCE(SUM(duration_ms), 0)
         FROM plays WHERE played_at >= ?1 AND artist IS NOT NULL AND artist != ''
         GROUP BY artist ORDER BY 2 DESC, 3 DESC LIMIT ?2",
        from,
        |row| {
            Ok(StatsEntry {
                name: row.get(0)?,
                detail: None,
                path: None,
                plays: row.get(1)?,
                listened_ms: row.get::<_, i64>(2)? as u64,
            })
        },
    )?;
    let top_albums = entries(
        connection,
        "SELECT album, artist, COUNT(*), COALESCE(SUM(duration_ms), 0)
         FROM plays WHERE played_at >= ?1 AND album IS NOT NULL AND album != ''
         GROUP BY album, artist ORDER BY 3 DESC, 4 DESC LIMIT ?2",
        from,
        |row| {
            Ok(StatsEntry {
                name: row.get(0)?,
                detail: row.get(1)?,
                path: None,
                plays: row.get(2)?,
                listened_ms: row.get::<_, i64>(3)? as u64,
            })
        },
    )?;

    let mut statement = connection.prepare(&format!(
        "SELECT strftime('{}', played_at, 'unixepoch', 'localtime') AS bucket,
                COUNT(*), COALESCE(SUM(duration_ms), 0)
         FROM plays WHERE played_at >= ?1 GROUP BY bucket ORDER BY bucket",
        period.bucket_format()
    ))?;
    let buckets = statement
        .query_map([from], |row| {
            Ok(StatsBucket {
                label: row.get(0)?,
                plays: row.get(1)?,
                listened_ms: row.get::<_, i64>(2)? as u64,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut by_hour = vec![0u32; 24];
    let mut statement = connection.prepare(
        "SELECT CAST(strftime('%H', played_at, 'unixepoch', 'localtime') AS INTEGER), COUNT(*)
         FROM plays WHERE played_at >= ?1 GROUP BY 1",
    )?;
    for row in statement.query_map([from], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, u32>(1)?)))? {
        let (hour, count) = row?;
        if let Some(slot) = usize::try_from(hour).ok().and_then(|hour| by_hour.get_mut(hour)) {
            *slot = count;
        }
    }

    Ok(ListeningStats {
        plays,
        listened_ms: listened_ms.max(0) as u64,
        tracks,
        artists,
        top_artists,
        top_tracks,
        top_albums,
        buckets,
        by_hour,
    })
}

fn entries(
    connection: &Connection,
    sql: &str,
    from: i64,
    map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<StatsEntry>,
) -> Result<Vec<StatsEntry>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map(rusqlite::params![from, TOP], map)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(path: &str, artist: &str, album: &str) -> PlayRecord {
        PlayRecord {
            path: path.into(),
            title: path.trim_end_matches(".flac").into(),
            artist: Some(artist.into()),
            album: Some(album.into()),
            duration_ms: 200_000,
        }
    }

    #[test]
    fn counts_tops_and_periods() {
        let path = std::env::temp_dir().join(format!("ringloft-stats-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = crate::library::db::Db::open(path.clone()).expect("база");
        let now = 1_790_000_000u64;
        db.with_write(|connection| {
            record(connection, &play("a.flac", "ЛСП", "Magic City"), now - 3600)?;
            record(connection, &play("a.flac", "ЛСП", "Magic City"), now - 7200)?;
            record(connection, &play("b.flac", "Joji", "Ballads 1"), now - 3600)?;
            // Сорок дней назад — в месяц уже не попадает.
            record(connection, &play("c.flac", "Joji", "Ballads 1"), now - 40 * 24 * 3600)
        })
        .expect("запись");

        let month = db
            .with_read(|connection| listening(connection, StatsPeriod::Month, now))
            .expect("итоги");
        assert_eq!(month.plays, 3);
        assert_eq!(month.listened_ms, 600_000);
        assert_eq!(month.tracks, 2);
        assert_eq!(month.artists, 2);
        assert_eq!(month.top_tracks[0].path.as_deref(), Some("a.flac"));
        assert_eq!(month.top_tracks[0].plays, 2);
        assert_eq!(month.top_artists[0].name, "ЛСП");
        assert_eq!(month.by_hour.iter().sum::<u32>(), 3);

        let all = db
            .with_read(|connection| listening(connection, StatsPeriod::All, now))
            .expect("итоги");
        assert_eq!(all.plays, 4);
        assert_eq!(all.top_artists.len(), 2);
        // У обоих альбомов по два прослушивания — ничья, порядок любой.
        assert_eq!(all.top_albums.len(), 2);
        assert!(all.top_albums.iter().all(|album| album.plays == 2));
        drop(db);
        let _ = std::fs::remove_file(&path);
    }
}
