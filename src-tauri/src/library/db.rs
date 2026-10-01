//! Соединения с SQLite, схема и миграции.
//!
//! Пишет только сканер (одно соединение под мьютексом), читают команды из
//! маленького пула. WAL позволяет читать во время записи, поэтому UI не ждёт
//! сканирование.

use std::path::PathBuf;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};

use crate::error::Result;

/// Версия схемы. Растёт вместе с миграциями в `migrate`.
const SCHEMA_VERSION: i64 = 3;
/// Сколько читающих соединений держим открытыми.
const READER_POOL: usize = 4;

pub struct Db {
    path: PathBuf,
    writer: Mutex<Connection>,
    readers: Mutex<Vec<Connection>>,
}

impl Db {
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| crate::error::RingloftError::io(dir, err))?;
        }

        let mut writer = Connection::open(&path)?;
        configure(&writer)?;
        migrate(&mut writer)?;

        Ok(Self {
            path,
            writer: Mutex::new(writer),
            readers: Mutex::new(Vec::new()),
        })
    }

    pub fn with_write<T>(&self, action: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut guard = self.writer.lock();
        action(&mut guard)
    }

    /// Соединение берётся из пула и возвращается обратно. Пул пустой — открываем
    /// новое: это дешевле, чем заставлять UI ждать сканер.
    pub fn with_read<T>(&self, action: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let connection = match self.readers.lock().pop() {
            Some(connection) => connection,
            None => {
                let connection = Connection::open_with_flags(
                    &self.path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                configure(&connection)?;
                connection
            }
        };

        let result = action(&connection);

        let mut pool = self.readers.lock();
        if pool.len() < READER_POOL {
            pool.push(connection);
        }
        result
    }
}

fn configure(connection: &Connection) -> Result<()> {
    // WAL — ради одновременного чтения во время сканирования.
    // NORMAL вместо FULL: потеря последних записей при отключении питания
    // не страшна, библиотека пересобирается сканированием.
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

fn migrate(connection: &mut Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    if version < 1 {
        transaction.execute_batch(SCHEMA_V1)?;
    }
    if version < 2 {
        transaction.execute_batch(SCHEMA_V2)?;
    }
    if version < 3 {
        transaction.execute_batch(SCHEMA_V3)?;
    }
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;

    tracing::info!(from = version, to = SCHEMA_VERSION, "схема библиотеки обновлена");
    Ok(())
}

/// `album_key` считается в Rust: это «исполнитель альбома + альбом» в нижнем
/// регистре. С ним галерея и дерево собираются одним GROUP BY, без склейки
/// строк в каждом запросе.
const SCHEMA_V1: &str = r#"
CREATE TABLE tracks (
    id            INTEGER PRIMARY KEY,
    path          TEXT    NOT NULL UNIQUE,
    folder        TEXT    NOT NULL,
    title         TEXT    NOT NULL,
    artist        TEXT,
    album_artist  TEXT,
    album         TEXT,
    album_key     TEXT,
    genre         TEXT,
    year          INTEGER,
    track_no      INTEGER,
    disc_no       INTEGER,
    duration_ms   INTEGER,
    sample_rate   INTEGER,
    channels      INTEGER,
    bitrate_kbps  INTEGER,
    size_bytes    INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    has_cover     INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL,
    scanned_at    INTEGER NOT NULL
);

CREATE INDEX idx_tracks_album   ON tracks(album_key, disc_no, track_no);
CREATE INDEX idx_tracks_artist  ON tracks(artist);
CREATE INDEX idx_tracks_folder  ON tracks(folder);
CREATE INDEX idx_tracks_genre   ON tracks(genre);

CREATE VIRTUAL TABLE tracks_fts USING fts5(
    title, artist, album, genre,
    content = 'tracks',
    content_rowid = 'id',
    tokenize = "unicode61 remove_diacritics 2"
);

CREATE TRIGGER tracks_ai AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, title, artist, album, genre)
    VALUES (new.id, new.title, new.artist, new.album, new.genre);
END;

CREATE TRIGGER tracks_ad AFTER DELETE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, genre)
    VALUES ('delete', old.id, old.title, old.artist, old.album, old.genre);
END;

CREATE TRIGGER tracks_au AFTER UPDATE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, genre)
    VALUES ('delete', old.id, old.title, old.artist, old.album, old.genre);
    INSERT INTO tracks_fts(rowid, title, artist, album, genre)
    VALUES (new.id, new.title, new.artist, new.album, new.genre);
END;
"#;

/// Статистика прослушиваний. Она живёт в базе, а не в тегах: писать в файл
/// на каждое прослушивание — это лишние килобайты записи и сбитый mtime,
/// из-за которого сканер стал бы перечитывать файл.
///
/// Заодно сужаем триггер FTS: он перестраивал поисковый индекс на **любое**
/// обновление строки, а теперь только на изменение полей, которые в нём и
/// лежат. Иначе каждый счётчик прослушиваний дёргал бы индекс.
const SCHEMA_V2: &str = r#"
ALTER TABLE tracks ADD COLUMN play_count  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tracks ADD COLUMN last_played INTEGER;
ALTER TABLE tracks ADD COLUMN rating      INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_tracks_played ON tracks(play_count, last_played);
CREATE INDEX idx_tracks_rating ON tracks(rating);

DROP TRIGGER tracks_au;
CREATE TRIGGER tracks_au AFTER UPDATE OF title, artist, album, genre ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, genre)
    VALUES ('delete', old.id, old.title, old.artist, old.album, old.genre);
    INSERT INTO tracks_fts(rowid, title, artist, album, genre)
    VALUES (new.id, new.title, new.artist, new.album, new.genre);
END;
"#;

/// История прослушиваний — для итогов. Строка на каждое засчитанное
/// прослушивание, с названием и исполнителем **на тот момент**: так в итоги
/// попадают и треки вне библиотеки, а пересканирование и правка тегов
/// прошлое не переписывают.
///
/// Накопленные до этого счётчики переносятся сюда же: каждое прослушивание —
/// на дату последнего (точнее данных нет, а итоги «за всё время» сойдутся).
const SCHEMA_V3: &str = r#"
CREATE TABLE plays (
    id          INTEGER PRIMARY KEY,
    played_at   INTEGER NOT NULL,
    path        TEXT    NOT NULL,
    title       TEXT    NOT NULL,
    artist      TEXT,
    album       TEXT,
    duration_ms INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_plays_time ON plays(played_at);

WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 10000)
INSERT INTO plays (played_at, path, title, artist, album, duration_ms)
SELECT t.last_played, t.path, t.title, t.artist, t.album, COALESCE(t.duration_ms, 0)
FROM tracks t JOIN n ON n.i <= t.play_count
WHERE t.play_count > 0 AND t.last_played IS NOT NULL;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (Db, PathBuf) {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ringloft-lib-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        ));
        let db = Db::open(path.clone()).expect("создание базы");
        (db, path)
    }

    #[test]
    fn bundled_sqlite_has_fts5() {
        let (db, path) = temp_db();
        let has_fts5 = db
            .with_read(|connection| {
                let mut statement = connection.prepare("PRAGMA compile_options")?;
                let mut rows = statement.query([])?;
                let mut found = false;
                while let Some(row) = rows.next()? {
                    let option: String = row.get(0)?;
                    if option.contains("FTS5") {
                        found = true;
                    }
                }
                Ok(found)
            })
            .expect("чтение compile_options");
        assert!(has_fts5, "в сборке SQLite нет FTS5");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migration_is_idempotent() {
        let (db, path) = temp_db();
        drop(db);
        // Повторное открытие не должно падать на уже созданных таблицах.
        let db = Db::open(path.clone()).expect("повторное открытие");
        let version: i64 = db
            .with_read(|connection| Ok(connection.query_row("PRAGMA user_version", [], |row| row.get(0))?))
            .expect("версия схемы");
        assert_eq!(version, SCHEMA_VERSION);
        let _ = std::fs::remove_file(&path);
    }
}
