//! Мелочи про текст: чтение файлов с неизвестной кодировкой и разбор строки
//! «Исполнитель - Трек».
//!
//! В коллекциях со стажем и `.m3u`, и `.lrc` регулярно приходят в cp1251:
//! их писали в Windows задолго до того, как UTF-8 стал нормой.

/// Таблица верхней половины cp1251: с 0x80 по 0xFF.
const CP1251_HIGH: [char; 128] = [
    'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', 'ђ', '‘', '’',
    '“', '”', '•', '–', '—', '\u{98}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', '\u{a0}', 'Ў', 'ў', 'Ј',
    '¤', 'Ґ', '¦', '§', 'Ё', '©', 'Є', '«', '¬', '\u{ad}', '®', 'Ї', '°', '±', 'І', 'і', 'ґ', 'µ',
    '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї', 'А', 'Б', 'В', 'Г', 'Д', 'Е', 'Ж', 'З', 'И',
    'Й', 'К', 'Л', 'М', 'Н', 'О', 'П', 'Р', 'С', 'Т', 'У', 'Ф', 'Х', 'Ц', 'Ч', 'Ш', 'Щ', 'Ъ', 'Ы',
    'Ь', 'Э', 'Ю', 'Я', 'а', 'б', 'в', 'г', 'д', 'е', 'ж', 'з', 'и', 'й', 'к', 'л', 'м', 'н', 'о',
    'п', 'р', 'с', 'т', 'у', 'ф', 'х', 'ц', 'ч', 'ш', 'щ', 'ъ', 'ы', 'ь', 'э', 'ю', 'я',
];

/// Тянуть ради одной однобайтовой кодировки целый крейт
/// перекодировок незачем: однобайтовая таблица занимает десяток строк.
fn decode_cp1251(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| match byte {
            0x00..=0x7f => *byte as char,
            other => CP1251_HIGH[usize::from(*other) - 0x80],
        })
        .collect()
}

/// UTF-8, а если не вышло — cp1251. Других вариантов в наших форматах не
/// встречалось.
pub fn decode(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => decode_cp1251(err.as_bytes()),
    }
}

/// «Исполнитель - Трек» — так называют трек и радиостанции в эфире, и файлы
/// в скачанных сборниках. Делим по **первому** тире с пробелами: дальше тире
/// обычно часть названия.
pub fn split_artist_title(value: &str) -> Option<(String, String)> {
    let (artist, title) = value.split_once(" - ")?;
    let artist = artist.trim();
    let title = title.trim();
    (!artist.is_empty() && !title.is_empty()).then(|| (artist.to_owned(), title.to_owned()))
}

/// `file:///home/…/%D0%A2%D1%80%D0%B5%D0%BA.flac` → путь. Так файлы
/// присылает MPRIS (`OpenUri`): кириллица и пробелы в адресе закодированы.
/// `None` — это не `file://` или после раскодирования вышла не строка.
// Нужно только MPRIS, то есть только на Linux.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn path_from_file_url(url: &str) -> Option<std::path::PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // `file://host/путь` встречается, но у нас хост всегда пустой или localhost.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);

    let bytes = rest.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = rest.get(index + 1..index + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    let path = String::from_utf8(decoded).ok()?;
    path.starts_with('/').then(|| std::path::PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_cp1251() {
        // «Трек.mp3» в cp1251.
        let bytes = vec![0xd2, 0xf0, 0xe5, 0xea, b'.', b'm', b'p', b'3'];
        assert_eq!(decode(bytes), "Трек.mp3");
        assert_eq!(decode("Трек".as_bytes().to_vec()), "Трек");
    }

    #[test]
    fn decodes_file_urls() {
        assert_eq!(
            path_from_file_url("file:///home/user/Music/%D0%A2%D1%80%D0%B5%D0%BA%201.flac"),
            Some(std::path::PathBuf::from("/home/user/Music/Трек 1.flac"))
        );
        assert_eq!(
            path_from_file_url("file://localhost/tmp/a.mp3"),
            Some(std::path::PathBuf::from("/tmp/a.mp3"))
        );
        // Не file:// и битые последовательности — не путь, а не паника.
        assert_eq!(path_from_file_url("https://example.org/a.mp3"), None);
        assert_eq!(path_from_file_url("file:///%FF%FE"), None);
        assert_eq!(
            path_from_file_url("file:///a%2"),
            Some(std::path::PathBuf::from("/a%2")),
            "неполная последовательность остаётся как есть"
        );
    }

    /// Строку делят тире с пробелами — и в эфире, и в именах файлов.
    /// Исполнитель обязателен: без него ни скробблить, ни искать текст.
    #[test]
    fn splits_artist_and_title() {
        assert_eq!(
            split_artist_title("Кишлак - Ночь"),
            Some(("Кишлак".to_owned(), "Ночь".to_owned()))
        );
        assert_eq!(
            split_artist_title("  A  -  B - C  "),
            Some(("A".to_owned(), "B - C".to_owned())),
            "делим по первому тире, остальное — часть названия"
        );
        assert_eq!(split_artist_title("Radio Paradise"), None);
        assert_eq!(split_artist_title(" - Ночь"), None);
        assert_eq!(split_artist_title("Кишлак - "), None);
    }
}
