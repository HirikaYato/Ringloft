//! Источник для symphonia, читающий поток по HTTP (интернет-радио).
//!
//! Две особенности, из-за которых нельзя просто отдать тело ответа:
//!
//! 1. Поток бесконечный и неперематываемый — `MediaSource` должен честно об
//!    этом сказать, иначе демуксер попытается искать конец файла.
//! 2. Если станция согласилась на `Icy-MetaData`, то **внутри звука** каждые
//!    `icy-metaint` байт лежит блок с названием трека. Не вырезать его —
//!    значит отдать декодеру мусор посреди кадра.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use symphonia::core::io::MediaSource;

use crate::error::{RingloftError, Result};

/// Сколько ждём соединения. Само чтение не ограничиваем: поток на то и поток.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Больше этого в блоке метаданных не бывает: длина в нём — один байт с
/// множителем 16.
const MAX_META: usize = 16 * 255;

/// Название трека, которое станция шлёт вместе со звуком.
pub type StreamTitle = Arc<Mutex<Option<String>>>;

pub struct HttpSource {
    reader: Box<dyn Read + Send + Sync>,
    /// Через сколько байт звука встретится блок метаданных.
    meta_interval: Option<usize>,
    /// Сколько байт звука осталось до следующего блока.
    until_meta: usize,
    title: StreamTitle,
}

/// Открывает поток. Возвращает источник, подсказку формата из `Content-Type`
/// и ячейку, куда складывается название трека.
pub fn open(url: &str) -> Result<(HttpSource, Option<String>, StreamTitle)> {
    let response = ureq::get(url)
        .header("Icy-MetaData", "1")
        .header("User-Agent", crate::net::USER_AGENT)
        .config()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .build()
        .call()
        .map_err(|err| RingloftError::Network(format!("{url}: {err}")))?;

    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    // Ноль от кривого сервера означал бы «блок метаданных после каждых нуля
    // байт» — чтение ушло бы в бесконечный цикл.
    let meta_interval = header("icy-metaint")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|interval| *interval > 0);
    let content_type = header("content-type");

    let title: StreamTitle = Arc::new(Mutex::new(header("icy-name")));
    let source = HttpSource {
        reader: Box::new(response.into_body().into_reader()),
        meta_interval,
        until_meta: meta_interval.unwrap_or(0),
        title: Arc::clone(&title),
    };
    Ok((source, content_type, title))
}

impl HttpSource {
    /// Для тестов: источник над готовым читателем.
    #[cfg(test)]
    fn from_reader(reader: Box<dyn Read + Send + Sync>, meta_interval: Option<usize>) -> Self {
        Self {
            reader,
            meta_interval,
            until_meta: meta_interval.unwrap_or(0),
            title: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    fn title(&self) -> Option<String> {
        self.title.lock().clone()
    }

    /// Снимает из потока блок метаданных: байт длины и сам блок.
    fn consume_meta(&mut self) -> io::Result<()> {
        let mut length = [0u8; 1];
        self.reader.read_exact(&mut length)?;
        let size = usize::from(length[0]) * 16;
        if size == 0 || size > MAX_META {
            return Ok(());
        }

        let mut block = vec![0u8; size];
        self.reader.read_exact(&mut block)?;
        if let Some(found) = parse_title(&block) {
            *self.title.lock() = Some(found);
        }
        Ok(())
    }
}

impl Read for HttpSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(interval) = self.meta_interval else {
            return self.reader.read(buf);
        };

        if self.until_meta == 0 {
            self.consume_meta()?;
            self.until_meta = interval;
        }
        // Читаем не больше, чем осталось до блока метаданных, иначе он
        // попадёт в звук.
        let want = buf.len().min(self.until_meta);
        let read = self.reader.read(&mut buf[..want])?;
        self.until_meta -= read;
        Ok(read)
    }
}

impl Seek for HttpSource {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "поток нельзя перематывать",
        ))
    }
}

impl MediaSource for HttpSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// `StreamTitle='Исполнитель - Трек';` — иногда с другими полями рядом.
fn parse_title(block: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(block);
    let start = text.find("StreamTitle=")? + "StreamTitle=".len();
    let rest = text[start..].trim_start();
    let quote = rest.chars().next()?;
    let rest = &rest[quote.len_utf8()..];
    let end = rest.find(quote)?;
    let title = rest[..end].trim();
    (!title.is_empty()).then(|| title.to_owned())
}

/// Подсказка формата по `Content-Type`: у потоков расширения нет.
pub fn extension_for(content_type: Option<&str>) -> Option<&'static str> {
    let kind = content_type?.split(';').next()?.trim().to_ascii_lowercase();
    Some(match kind.as_str() {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/aac" | "audio/aacp" => "aac",
        "audio/mp4" | "audio/m4a" => "m4a",
        "audio/ogg" | "application/ogg" | "audio/vorbis" => "ogg",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Блок метаданных лежит прямо в звуке: декодеру он достаться не должен,
    /// а название — должно.
    #[test]
    fn strips_icy_metadata_from_audio() {
        let interval = 8;
        let mut raw = Vec::new();
        raw.extend_from_slice(&[1u8; 8]);
        let meta = b"StreamTitle='Kishlak - Noch';";
        let padded = {
            let mut block = meta.to_vec();
            block.resize(meta.len().div_ceil(16) * 16, 0);
            block
        };
        raw.push((padded.len() / 16) as u8);
        raw.extend_from_slice(&padded);
        raw.extend_from_slice(&[2u8; 8]);
        // Пустой блок метаданных — тоже законный случай.
        raw.push(0);
        raw.extend_from_slice(&[3u8; 4]);

        let mut source = HttpSource::from_reader(Box::new(Cursor::new(raw)), Some(interval));
        let mut audio = Vec::new();
        source.read_to_end(&mut audio).expect("чтение");

        assert_eq!(audio, [[1u8; 8], [2u8; 8]].concat().into_iter().chain([3u8; 4]).collect::<Vec<u8>>());
        assert_eq!(source.title().as_deref(), Some("Kishlak - Noch"));
    }

    /// Нулевой интервал — это не «метаданные каждые ноль байт», а мусор в
    /// заголовке: читаем поток как обычный.
    #[test]
    fn zero_metaint_is_ignored() {
        let mut source = HttpSource::from_reader(Box::new(Cursor::new(vec![5u8; 12])), None);
        let mut audio = Vec::new();
        source.read_to_end(&mut audio).expect("чтение");
        assert_eq!(audio.len(), 12);
    }

    #[test]
    fn without_metaint_passes_bytes_through() {
        let mut source = HttpSource::from_reader(Box::new(Cursor::new(vec![7u8; 16])), None);
        let mut audio = Vec::new();
        source.read_to_end(&mut audio).expect("чтение");
        assert_eq!(audio, vec![7u8; 16]);
        assert_eq!(source.title(), None);
    }

    #[test]
    fn reads_title_in_either_quotes() {
        assert_eq!(
            parse_title(b"StreamTitle='A - B';StreamUrl='';").as_deref(),
            Some("A - B")
        );
        assert_eq!(
            parse_title(b"StreamTitle=\"A - B\";").as_deref(),
            Some("A - B")
        );
        assert_eq!(parse_title(b"StreamTitle='';").as_deref(), None);
        assert_eq!(parse_title(b"\0\0\0").as_deref(), None);
    }

    #[test]
    fn maps_content_type_to_extension() {
        assert_eq!(extension_for(Some("audio/mpeg")), Some("mp3"));
        assert_eq!(extension_for(Some("audio/aacp; charset=utf-8")), Some("aac"));
        assert_eq!(extension_for(Some("application/octet-stream")), None);
        assert_eq!(extension_for(None), None);
    }
}
