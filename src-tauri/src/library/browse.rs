//! Просмотр библиотеки: исполнители, альбомы, жанры, папки.
//!
//! Сортировка делается в Rust, а не в SQL: без ICU у SQLite `lower()` и
//! `COLLATE NOCASE` понимают только латиницу, и «Ямал» оказывался бы выше
//! «альбом». Списки тут в тысячи строк, сортировка на их фоне бесплатна.

use rusqlite::{Connection, params};
use serde::Serialize;
use ts_rs::TS;

use super::models::LibraryTrack;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "AlbumRow.ts")]
pub struct AlbumRow {
    pub key: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<u32>,
    pub track_count: u32,
    pub duration_ms: u64,
    pub has_cover: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "NamedCount.ts")]
pub struct NamedCount {
    pub name: String,
    pub tracks: u32,
    /// Для исполнителей — число альбомов, для остальных 0.
    pub albums: u32,
}

pub fn artists(connection: &Connection) -> Result<Vec<NamedCount>> {
    let mut statement = connection.prepare_cached(
        "SELECT COALESCE(NULLIF(album_artist, ''), NULLIF(artist, '')) AS name,
                COUNT(*), COUNT(DISTINCT album_key)
         FROM tracks
         WHERE COALESCE(NULLIF(album_artist, ''), NULLIF(artist, '')) IS NOT NULL
         GROUP BY name",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(NamedCount {
            name: row.get(0)?,
            tracks: row.get::<_, i64>(1)? as u32,
            albums: row.get::<_, i64>(2)? as u32,
        })
    })?;
    collect_sorted(rows, |item| item.name.clone())
}

pub fn genres(connection: &Connection) -> Result<Vec<NamedCount>> {
    let mut statement = connection.prepare_cached(
        "SELECT genre, COUNT(*) FROM tracks
         WHERE genre IS NOT NULL AND genre <> ''
         GROUP BY genre",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(NamedCount {
            name: row.get(0)?,
            tracks: row.get::<_, i64>(1)? as u32,
            albums: 0,
        })
    })?;
    collect_sorted(rows, |item| item.name.clone())
}

pub fn folders(connection: &Connection) -> Result<Vec<NamedCount>> {
    let mut statement =
        connection.prepare_cached("SELECT folder, COUNT(*) FROM tracks GROUP BY folder")?;
    let rows = statement.query_map([], |row| {
        Ok(NamedCount {
            name: row.get(0)?,
            tracks: row.get::<_, i64>(1)? as u32,
            albums: 0,
        })
    })?;
    collect_sorted(rows, |item| item.name.clone())
}

/// Альбомы целиком или только одного исполнителя.
pub fn albums(connection: &Connection, artist: Option<&str>) -> Result<Vec<AlbumRow>> {
    let sql = "SELECT album_key,
                      MAX(album),
                      MAX(COALESCE(NULLIF(album_artist, ''), NULLIF(artist, ''))),
                      MAX(year),
                      COUNT(*),
                      COALESCE(SUM(duration_ms), 0),
                      MAX(has_cover)
               FROM tracks
               WHERE album_key IS NOT NULL";

    let mut rows = Vec::new();
    let push = |row: &rusqlite::Row<'_>| -> rusqlite::Result<AlbumRow> {
        Ok(AlbumRow {
            key: row.get(0)?,
            title: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            artist: row.get(2)?,
            year: row.get::<_, Option<i64>>(3)?.map(|value| value as u32),
            track_count: row.get::<_, i64>(4)? as u32,
            duration_ms: row.get::<_, i64>(5)? as u64,
            has_cover: row.get::<_, i64>(6)? != 0,
        })
    };

    match artist {
        Some(artist) => {
            let sql = format!(
                "{sql} AND COALESCE(NULLIF(album_artist, ''), NULLIF(artist, '')) = ?1 GROUP BY album_key"
            );
            let mut statement = connection.prepare_cached(&sql)?;
            for row in statement.query_map(params![artist], |row| push(row))? {
                rows.push(row?);
            }
        }
        None => {
            let sql = format!("{sql} GROUP BY album_key");
            let mut statement = connection.prepare_cached(&sql)?;
            for row in statement.query_map([], |row| push(row))? {
                rows.push(row?);
            }
        }
    }

    rows.sort_by_key(|album| {
        (
            album
                .artist
                .as_deref()
                .unwrap_or("")
                .to_lowercase(),
            album.year.unwrap_or(0),
            album.title.to_lowercase(),
        )
    });
    Ok(rows)
}

/// Треки одного альбома, жанра, исполнителя или папки — в порядке для показа.
pub fn tracks_of(connection: &Connection, filter: &TrackFilter) -> Result<Vec<LibraryTrack>> {
    let (clause, value) = match filter {
        TrackFilter::Album(key) => ("album_key = ?1", key.as_str()),
        TrackFilter::Artist(name) => (
            "COALESCE(NULLIF(album_artist, ''), NULLIF(artist, '')) = ?1",
            name.as_str(),
        ),
        TrackFilter::Genre(name) => ("genre = ?1", name.as_str()),
        TrackFilter::Folder(path) => ("folder = ?1", path.as_str()),
    };

    let sql = format!(
        "SELECT {columns}
         FROM tracks WHERE {clause}
         ORDER BY album_key, disc_no, track_no, title
         LIMIT 5000",
        columns = super::models::TRACK_COLUMNS
    );

    let mut statement = connection.prepare_cached(&sql)?;
    let rows = statement.query_map(params![value], LibraryTrack::from_row)?;

    let mut tracks = Vec::new();
    for row in rows {
        tracks.push(row?);
    }
    Ok(tracks)
}

#[derive(Debug, Clone)]
pub enum TrackFilter {
    Album(String),
    Artist(String),
    Genre(String),
    Folder(String),
}

fn collect_sorted<T>(
    rows: impl Iterator<Item = rusqlite::Result<T>>,
    key: impl Fn(&T) -> String,
) -> Result<Vec<T>> {
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    items.sort_by_cached_key(|item| key(item).to_lowercase());
    Ok(items)
}
