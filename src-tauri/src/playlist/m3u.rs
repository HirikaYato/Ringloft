//! Импорт и экспорт M3U/M3U8.
//!
//! Формат простой до неприличия, но в жизни встречается всякое: BOM, обратные
//! слэши из Windows, относительные пути, комментарии, ссылки на потоки и
//! кодировка cp1251 в старых `.m3u`. Всё это тут и разбирается.

use std::path::{Path, PathBuf};

use super::PlaylistItem;

/// Разбирает содержимое списка. Пути приводятся к абсолютным относительно
/// папки самого файла; строки, которые не похожи на локальный файл, пропускаем.
pub fn parse(content: &str, base_dir: &Path) -> Vec<PathBuf> {
    content
        .lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            if is_http(line) {
                // Интернет-радио оставляем как есть: это адрес, а не путь.
                return Some(PathBuf::from(line));
            }
            // Прочие схемы (mms, rtsp) плеер не умеет — молча пропускаем.
            (!is_remote(line)).then(|| resolve(line, base_dir))
        })
        .collect()
}

/// Собирает M3U8 с расширенными строками. Пути внутри папки списка пишем
/// относительными — так плейлист переживает переезд каталога.
pub fn render(items: &[&PlaylistItem], target_dir: &Path) -> String {
    let mut out = String::from("#EXTM3U\n");
    for item in items {
        let seconds = item
            .duration_ms
            .map(|ms| (ms as f64 / 1000.0).round() as i64)
            .unwrap_or(-1);
        let title = match item.artist.as_deref() {
            Some(artist) if !artist.is_empty() => format!("{artist} - {}", item.title),
            _ => item.title.clone(),
        };
        out.push_str(&format!("#EXTINF:{seconds},{title}\n"));
        out.push_str(&relative_to(&item.path, target_dir));
        out.push('\n');
    }
    out
}

fn is_http(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn is_remote(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mms://")
        || lower.starts_with("rtsp://")
}

fn resolve(line: &str, base_dir: &Path) -> PathBuf {
    // Списки из Windows приходят с обратными слэшами даже на Linux.
    let normalized = line.replace('\\', "/");
    let path = PathBuf::from(&normalized);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn relative_to(path: &Path, base_dir: &Path) -> String {
    path.strip_prefix(base_dir)
        .map(|relative| relative.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extended_playlist() {
        let content = "\u{feff}#EXTM3U\n\
                       #EXTINF:213,Kaze no Oto - Первый снег\n\
                       album/01.flac\n\
                       \n\
                       # просто комментарий\n\
                       /music/absolute.flac\n\
                       http://stream.example/radio\n\
                       windows\\path\\track.mp3\n";
        let paths = parse(content, Path::new("/base"));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/base/album/01.flac"),
                PathBuf::from("/music/absolute.flac"),
                PathBuf::from("http://stream.example/radio"),
                PathBuf::from("/base/windows/path/track.mp3"),
            ],
            "адрес потока остаётся адресом, относительные пути разворачиваем"
        );
    }

    #[test]
    fn renders_relative_paths_and_durations() {
        let mut item = PlaylistItem::from_path(1, PathBuf::from("/base/album/01.flac"));
        item.title = "Первый снег".into();
        item.artist = Some("Kaze no Oto".into());
        item.duration_ms = Some(213_400);

        let text = render(&[&item], Path::new("/base"));
        assert!(text.starts_with("#EXTM3U\n"));
        assert!(text.contains("#EXTINF:213,Kaze no Oto - Первый снег\n"));
        assert!(text.contains("\nalbum/01.flac\n"), "путь внутри папки — относительный");
    }

    /// Интернет-радио из списка теперь играется, а неподдерживаемые схемы
    /// по-прежнему отбрасываются.
    #[test]
    fn keeps_http_streams_and_drops_other_schemes() {
        let content = "#EXTM3U\nhttp://stream.example/live\nmms://old.example/x\ntrack.flac\n";
        let paths = parse(content, Path::new("/music"));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("http://stream.example/live"),
                PathBuf::from("/music/track.flac"),
            ]
        );
    }

    #[test]
    fn survives_cp1251() {
        // «Трек.mp3» в cp1251.
        let bytes = vec![
            0xd2, 0xf0, 0xe5, 0xea, b'.', b'm', b'p', b'3', b'\n',
        ];
        let text = crate::text::decode(bytes);
        assert_eq!(text.trim(), "Трек.mp3");
    }

    #[test]
    fn round_trip_keeps_files() {
        let mut item = PlaylistItem::from_path(1, PathBuf::from("/base/a b/c.flac"));
        item.duration_ms = Some(1000);
        let text = render(&[&item], Path::new("/base"));
        let paths = parse(&text, Path::new("/base"));
        assert_eq!(paths, vec![PathBuf::from("/base/a b/c.flac")]);
    }
}
