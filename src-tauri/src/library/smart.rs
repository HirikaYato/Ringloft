//! Умные списки: набор условий вместо ручного перечисления треков.
//!
//! Условия приходят с фронта, поэтому в SQL не попадает **ничего** из них
//! дословно: поле и операция выбираются из закрытых перечислений, а значение
//! всегда уезжает параметром.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::models::{LibraryTrack, TRACK_COLUMNS};
use crate::error::Result;

/// Больше этого умный список не набирает: он для слушания, а не для выгрузки.
const MAX_TRACKS: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SmartField.ts")]
pub enum SmartField {
    Title,
    Artist,
    Album,
    Genre,
    Year,
    Rating,
    PlayCount,
    LastPlayed,
    AddedAt,
    Duration,
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SmartOp.ts")]
pub enum SmartOp {
    /// Подстрока без учёта регистра.
    Contains,
    Is,
    IsNot,
    Greater,
    Less,
    /// Пусто: ни разу не слушали, нет жанра, нет оценки.
    Empty,
    /// За последние N дней.
    WithinDays,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SmartSort.ts")]
pub enum SmartSort {
    Added,
    LastPlayed,
    PlayCount,
    Rating,
    Artist,
    Title,
    Year,
    Random,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SmartRule.ts")]
pub struct SmartRule {
    pub field: SmartField,
    pub op: SmartOp,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "SmartRules.ts")]
pub struct SmartRules {
    /// Все условия должны совпасть: «или» в жизни почти не нужно, а вид
    /// правил усложняет заметно.
    pub rules: Vec<SmartRule>,
    pub sort: SmartSort,
    pub descending: bool,
    pub limit: Option<u32>,
}

impl Default for SmartRules {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            sort: SmartSort::Added,
            descending: true,
            limit: None,
        }
    }
}

pub fn tracks(connection: &Connection, rules: &SmartRules) -> Result<Vec<LibraryTrack>> {
    let (where_sql, params) = build_where(rules);
    let limit = rules.limit.unwrap_or(MAX_TRACKS).clamp(1, MAX_TRACKS);

    let sql = format!(
        "SELECT {TRACK_COLUMNS} FROM tracks WHERE {where_sql} ORDER BY {} LIMIT {limit}",
        order_by(rules)
    );

    let mut statement = connection.prepare(&sql)?;
    let bound: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|value| value as &dyn rusqlite::ToSql).collect();
    let rows = statement.query_map(bound.as_slice(), LibraryTrack::from_row)?;

    let mut found = Vec::new();
    for row in rows {
        found.push(row?);
    }
    Ok(found)
}

/// Значение параметра: числа и строки ходят по-разному.
#[derive(Debug)]
enum Value {
    Text(String),
    Number(i64),
}

impl rusqlite::ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            Value::Text(text) => text.to_sql(),
            Value::Number(number) => number.to_sql(),
        }
    }
}

fn build_where(rules: &SmartRules) -> (String, Vec<Value>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();

    for rule in &rules.rules {
        if let Some(clause) = clause_for(rule, &mut params) {
            clauses.push(clause);
        }
    }

    if clauses.is_empty() {
        return ("1".to_owned(), params);
    }
    (clauses.join(" AND "), params)
}

fn clause_for(rule: &SmartRule, params: &mut Vec<Value>) -> Option<String> {
    let column = column_of(rule.field);
    let numeric = is_numeric(rule.field);
    let index = params.len() + 1;

    match rule.op {
        SmartOp::Empty => Some(if numeric {
            // Ноль и NULL для счётчика и оценки значат одно и то же.
            format!("COALESCE({column}, 0) = 0")
        } else {
            format!("COALESCE({column}, '') = ''")
        }),
        SmartOp::Contains => {
            params.push(Value::Text(format!("%{}%", rule.value.trim())));
            Some(format!("{column} LIKE ?{index} ESCAPE '\\'"))
        }
        SmartOp::Is | SmartOp::IsNot | SmartOp::Greater | SmartOp::Less => {
            let value = if numeric {
                Value::Number(rule.value.trim().parse().ok()?)
            } else {
                Value::Text(rule.value.trim().to_owned())
            };
            params.push(value);
            let operator = match rule.op {
                SmartOp::Is => "=",
                SmartOp::IsNot => "<>",
                SmartOp::Greater => ">",
                _ => "<",
            };
            Some(format!("{column} {operator} ?{index}"))
        }
        SmartOp::WithinDays => {
            let days: i64 = rule.value.trim().parse().ok()?;
            let since = now_unix().saturating_sub(days.max(0) as u64 * 86_400);
            params.push(Value::Number(since as i64));
            Some(format!("{column} >= ?{index}"))
        }
    }
}

/// Поле и колонка сопоставляются таблицей, а не строкой из запроса.
fn column_of(field: SmartField) -> &'static str {
    match field {
        SmartField::Title => "title",
        SmartField::Artist => "COALESCE(NULLIF(album_artist, ''), artist)",
        SmartField::Album => "album",
        SmartField::Genre => "genre",
        SmartField::Year => "year",
        SmartField::Rating => "rating",
        SmartField::PlayCount => "play_count",
        SmartField::LastPlayed => "last_played",
        SmartField::AddedAt => "added_at",
        SmartField::Duration => "duration_ms",
        SmartField::Folder => "folder",
    }
}

fn is_numeric(field: SmartField) -> bool {
    matches!(
        field,
        SmartField::Year
            | SmartField::Rating
            | SmartField::PlayCount
            | SmartField::LastPlayed
            | SmartField::AddedAt
            | SmartField::Duration
    )
}

fn order_by(rules: &SmartRules) -> String {
    if rules.sort == SmartSort::Random {
        return "RANDOM()".to_owned();
    }
    let column = match rules.sort {
        SmartSort::Added => "added_at",
        SmartSort::LastPlayed => "last_played",
        SmartSort::PlayCount => "play_count",
        SmartSort::Rating => "rating",
        SmartSort::Artist => "COALESCE(NULLIF(album_artist, ''), artist)",
        SmartSort::Title => "title",
        SmartSort::Year => "year",
        SmartSort::Random => "RANDOM()",
    };
    let direction = if rules.descending { "DESC" } else { "ASC" };
    // Пустые значения в конец: список «по оценке» не должен начинаться с
    // неоценённого.
    format!("{column} IS NULL, {column} {direction}, title")
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
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
                    album_artist TEXT, album TEXT, album_key TEXT, genre TEXT, year INTEGER,
                    track_no INTEGER, disc_no INTEGER, duration_ms INTEGER, folder TEXT,
                    added_at INTEGER NOT NULL DEFAULT 0,
                    play_count INTEGER NOT NULL DEFAULT 0,
                    last_played INTEGER,
                    rating INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO tracks (path, title, artist, genre, year, play_count, rating, added_at)
                VALUES ('/m/1.flac', 'Раз', 'A', 'рэп', 2020, 0, 0, 100),
                       ('/m/2.flac', 'Два', 'B', 'рок', 2024, 7, 5, 200),
                       ('/m/3.flac', 'Три', 'C', 'рэп', 2024, 3, 4, 300);",
            )
            .expect("схема");
        connection
    }

    fn titles(connection: &Connection, rules: &SmartRules) -> Vec<String> {
        tracks(connection, rules)
            .expect("выборка")
            .into_iter()
            .map(|track| track.title)
            .collect()
    }

    /// Пустой набор условий — это вся библиотека, а не пустой список.
    #[test]
    fn no_rules_take_everything() {
        let connection = db();
        assert_eq!(titles(&connection, &SmartRules::default()).len(), 3);
    }

    /// Условия складываются по «и», значение уезжает параметром.
    #[test]
    fn combines_rules() {
        let connection = db();
        let rules = SmartRules {
            rules: vec![
                SmartRule {
                    field: SmartField::Genre,
                    op: SmartOp::Is,
                    value: "рэп".to_owned(),
                },
                SmartRule {
                    field: SmartField::Year,
                    op: SmartOp::Greater,
                    value: "2021".to_owned(),
                },
            ],
            sort: SmartSort::Title,
            descending: false,
            limit: None,
        };
        assert_eq!(titles(&connection, &rules), ["Три"]);
    }

    /// «Ни разу не слушал» и «любимое» — главные умные списки.
    #[test]
    fn finds_never_played_and_favourites() {
        let connection = db();
        let never = SmartRules {
            rules: vec![SmartRule {
                field: SmartField::PlayCount,
                op: SmartOp::Empty,
                value: String::new(),
            }],
            ..SmartRules::default()
        };
        assert_eq!(titles(&connection, &never), ["Раз"]);

        let loved = SmartRules {
            rules: vec![SmartRule {
                field: SmartField::Rating,
                op: SmartOp::Greater,
                value: "3".to_owned(),
            }],
            sort: SmartSort::Rating,
            descending: true,
            limit: None,
        };
        assert_eq!(titles(&connection, &loved), ["Два", "Три"]);
    }

    /// Нечисловое значение в числовом поле — не повод отдать всю библиотеку
    /// или уронить запрос: правило просто выбрасывается.
    #[test]
    fn ignores_broken_rule() {
        let connection = db();
        let rules = SmartRules {
            rules: vec![SmartRule {
                field: SmartField::Year,
                op: SmartOp::Greater,
                value: "две тысячи".to_owned(),
            }],
            ..SmartRules::default()
        };
        assert_eq!(titles(&connection, &rules).len(), 3);
    }

    /// Строка из запроса в SQL не попадает: кавычки и точка с запятой
    /// остаются обычным текстом.
    #[test]
    fn value_stays_a_parameter() {
        let connection = db();
        let rules = SmartRules {
            rules: vec![SmartRule {
                field: SmartField::Title,
                op: SmartOp::Contains,
                value: "'; DROP TABLE tracks; --".to_owned(),
            }],
            ..SmartRules::default()
        };
        assert!(titles(&connection, &rules).is_empty());
        // Таблица на месте.
        assert_eq!(titles(&connection, &SmartRules::default()).len(), 3);
    }

    #[test]
    fn limit_is_capped() {
        let connection = db();
        let rules = SmartRules {
            limit: Some(1),
            ..SmartRules::default()
        };
        assert_eq!(titles(&connection, &rules).len(), 1);
    }
}
