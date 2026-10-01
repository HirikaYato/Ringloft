//! Чтение файла через symphonia: демуксер + декодер + выдача interleaved f32.
//!
//! Наружу торчит один метод `next_block()`, который отдаёт очередную порцию
//! сэмплов в формате устройства (f32, interleaved). Битые пакеты пропускаются:
//! один плохой фрейм не должен обрывать трек.

use std::fs::File;
use std::path::Path;
/// Кодеки symphonia плюс Opus через libopus: своего у symphonia нет, а
/// голосовые из Telegram, звук с YouTube и часть радиостанций — это Opus.
fn codecs() -> &'static symphonia::core::codecs::registry::CodecRegistry {
    static CODECS: std::sync::OnceLock<symphonia::core::codecs::registry::CodecRegistry> =
        std::sync::OnceLock::new();
    CODECS.get_or_init(|| {
        let mut registry = symphonia::core::codecs::registry::CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
        registry
    })
}

#[cfg(test)]
use std::path::PathBuf;

use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use super::http_source::{self, StreamTitle};
use super::is_stream;
use crate::error::{RingloftError, Result};

/// Сколько битых пакетов подряд терпим, прежде чем считать файл безнадёжным.
const MAX_CONSECUTIVE_ERRORS: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpec {
    pub sample_rate: u32,
    pub channels: usize,
}

pub struct TrackSource {
    reader: Box<dyn FormatReader + 'static>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    spec: SourceSpec,
    duration_ms: Option<u64>,
    codec_name: &'static str,
    errors: u32,
    finished: bool,
    /// Кусок файла, который нас интересует. Для CUE это один трек внутри
    /// общего образа диска; для обычного файла — весь файл.
    region_start_ms: u64,
    region_frames: Option<u64>,
    produced_frames: u64,
    /// Демуксер встаёт на границу пакета, а не точно на запрошенную позицию.
    /// Разницу добираем, выбрасывая лишние кадры из первых блоков.
    skip_frames: u64,
    /// Кадр, на который нас просили встать. Сравниваем с меткой первого
    /// пакета после перемотки: считать в миллисекундах нельзя — округление
    /// уводит границу куска на десятки кадров.
    pending_seek_frame: Option<u64>,
    /// Название трека от станции: у радио его шлют в самом потоке.
    title: Option<StreamTitle>,
    /// Можно ли вообще перематывать. У потока нельзя, и просить нельзя тоже:
    /// symphonia стала бы «доезжать» до позиции чтением, а это минуты в
    /// заблокированном движке.
    seekable: bool,
}

impl TrackSource {
    /// Открывает файл целиком.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_region(path, 0, None)
    }

    /// Открывает кусок файла: от `start_ms` и до `end_ms` (или до конца).
    pub fn open_region(path: &Path, start_ms: u64, end_ms: Option<u64>) -> Result<Self> {
        let mut source = Self::open_inner(path)?;

        let region_ms = end_ms.map(|end| end.saturating_sub(start_ms));
        source.region_start_ms = start_ms;
        source.region_frames =
            region_ms.map(|ms| ms * u64::from(source.spec.sample_rate) / 1000);
        if let Some(ms) = region_ms {
            source.duration_ms = Some(ms);
        }

        if start_ms > 0 {
            source.seek_absolute(start_ms)?;
        }
        Ok(source)
    }

    fn open_inner(path: &Path) -> Result<Self> {
        // У радио нет ни файла, ни расширения: источник другой, а подсказку
        // формата даёт `Content-Type`.
        let (source, extension, title): (Box<dyn MediaSource>, Option<String>, _) =
            if is_stream(path) {
                let (stream, content_type, title) = http_source::open(&path.to_string_lossy())?;
                let extension =
                    http_source::extension_for(content_type.as_deref()).map(str::to_owned);
                (Box::new(stream), extension, Some(title))
            } else {
                let file = File::open(path).map_err(|err| match err.kind() {
                    std::io::ErrorKind::NotFound => RingloftError::NotFound(path.to_path_buf()),
                    _ => RingloftError::io(path, err),
                })?;
                let extension = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(str::to_owned);
                (Box::new(file), extension, None)
            };

        let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = extension.as_deref() {
            hint.with_extension(ext);
        }

        let reader = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|err| map_open_error(&err))?;

        let track = reader
            .default_track(TrackType::Audio)
            .ok_or_else(|| RingloftError::UnsupportedFormat("в файле нет аудиодорожки".into()))?;

        let track_id = track.id;
        let time_base = track.time_base;
        let duration_ms = track
            .time_base
            .zip(track.duration)
            .and_then(|(base, duration)| base.calc_duration(duration))
            .map(|time| time.as_millis().max(0) as u64);

        let Some(CodecParameters::Audio(params)) = track.codec_params.as_ref() else {
            return Err(RingloftError::UnsupportedFormat(
                "у дорожки неизвестный кодек".into(),
            ));
        };

        let sample_rate = params
            .sample_rate
            .ok_or_else(|| RingloftError::UnsupportedFormat("не указана частота дискретизации".into()))?;
        let channels = params
            .channels
            .as_ref()
            .map(|channels| channels.count())
            .filter(|count| *count > 0)
            .ok_or_else(|| RingloftError::UnsupportedFormat("не указаны каналы".into()))?;

        let decoder = codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|err| RingloftError::UnsupportedFormat(err.to_string()))?;
        let codec_name = decoder.codec_info().short_name;

        Ok(Self {
            reader,
            decoder,
            track_id,
            time_base,
            spec: SourceSpec {
                sample_rate,
                channels,
            },
            duration_ms,
            codec_name,
            errors: 0,
            finished: false,
            region_start_ms: 0,
            region_frames: None,
            produced_frames: 0,
            skip_frames: 0,
            pending_seek_frame: None,
            seekable: title.is_none(),
            title,
        })
    }

    /// Название трека от радиостанции, если оно пришло в потоке.
    pub fn stream_title(&self) -> Option<String> {
        self.title.as_ref().and_then(|title| title.lock().clone())
    }

    pub fn spec(&self) -> SourceSpec {
        self.spec
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    pub fn codec_name(&self) -> &'static str {
        self.codec_name
    }

    /// Перематывает на `position_ms`, возвращает позицию, куда реально попали.
    ///
    /// Демуксер отдаёт ближайший кадр не позже запрошенного, поэтому фактическая
    /// позиция может немного отличаться — её и показываем, чтобы счётчик времени
    /// не расходился со звуком.
    /// Перемотка внутри куска: позиция отсчитывается от его начала.
    pub fn seek(&mut self, position_ms: u64) -> Result<u64> {
        if !self.seekable {
            // Радио не перематывается: отвечаем тем, что уже сыграно.
            let rate = u64::from(self.spec.sample_rate).max(1);
            return Ok(self.produced_frames * 1000 / rate);
        }
        let absolute = self.seek_absolute(self.region_start_ms + position_ms)?;
        Ok(absolute.saturating_sub(self.region_start_ms))
    }

    /// Перемотка по абсолютной позиции в файле.
    ///
    /// Демуксер может встать и раньше запрошенного (тогда лишние кадры
    /// выбрасываем), и позже (тогда просто считаем, что часть куска уже
    /// проехали) — иначе конец куска уползал бы на ту же величину.
    fn seek_absolute(&mut self, position_ms: u64) -> Result<u64> {
        self.seek_raw(position_ms)?;

        let rate = u64::from(self.spec.sample_rate);
        let target = position_ms * rate / 1000;
        let region_start = self.region_start_ms * rate / 1000;

        self.pending_seek_frame = Some(target);
        self.skip_frames = 0;
        self.produced_frames = target.saturating_sub(region_start);

        Ok(position_ms)
    }

    /// Метка пакета в кадрах. Для аудио временная база обычно 1/частота,
    /// но полагаться на это нельзя.
    fn pts_to_frames(&self, pts: symphonia::core::units::Timestamp) -> u64 {
        let ticks = pts.get().max(0) as u64;
        match self.time_base {
            Some(base) => {
                ticks * u64::from(base.numer.get()) * u64::from(self.spec.sample_rate)
                    / u64::from(base.denom.get())
            }
            None => ticks,
        }
    }

    fn seek_raw(&mut self, position_ms: u64) -> Result<u64> {
        let seeked = self
            .reader
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::from_millis_u64(position_ms),
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|err| RingloftError::Decode(format!("перемотка: {err}")))?;

        // После прыжка декодер обязан забыть контекст предыдущих кадров.
        self.decoder.reset();
        self.finished = false;
        self.errors = 0;

        Ok(self
            .time_base
            .and_then(|base| base.calc_time(seeked.actual_ts))
            .map(|time| time.as_millis().max(0) as u64)
            .unwrap_or(position_ms))
    }

    /// Кусок файла закончился — дальше читать незачем.
    fn region_exhausted(&self) -> bool {
        self.region_frames
            .is_some_and(|limit| self.produced_frames >= limit)
    }

    /// Декодирует следующий блок в `out` (interleaved f32).
    ///
    /// `out` очищается. `Ok(false)` — трек кончился. Буфер передаётся снаружи,
    /// чтобы не копировать сэмплы лишний раз и не отдавать наружу ссылку на
    /// внутренности декодера.
    pub fn next_block(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        out.clear();
        if self.finished || self.region_exhausted() {
            self.finished = true;
            return Ok(false);
        }

        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => {
                    self.finished = true;
                    return Ok(false);
                }
                Err(SymphoniaError::ResetRequired) => {
                    // Смена параметров потока посреди файла. Для шага 2 это конец:
                    // корректная обработка появится вместе с пересозданием декодера.
                    tracing::warn!("поток требует сброса декодера, останавливаю трек");
                    self.finished = true;
                    return Ok(false);
                }
                // Хвост файла обрезан: у голосовых из Telegram последняя страница
                // Ogg бывает неполной, у недокачанного mp3 — последний кадр.
                // Всё, что было, уже сыграно, — это конец трека, а не поломка.
                Err(SymphoniaError::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    tracing::debug!("файл кончился раньше, чем обещал контейнер");
                    self.finished = true;
                    return Ok(false);
                }
                Err(err) => {
                    self.finished = true;
                    return Err(RingloftError::Decode(err.to_string()));
                }
            };

            if packet.track_id != self.track_id {
                continue;
            }

            // Первый пакет после перемотки говорит, где мы оказались на самом деле.
            if let Some(target) = self.pending_seek_frame.take() {
                let packet_start = self.pts_to_frames(packet.pts);
                self.skip_frames = target.saturating_sub(packet_start);
            }

            match self.decoder.decode(&packet) {
                Ok(buffer) => {
                    self.errors = 0;
                    if buffer.is_empty() {
                        continue;
                    }
                    buffer.copy_to_vec_interleaved(out);

                    let channels = self.spec.channels.max(1);
                    if self.skip_frames > 0 {
                        let have = (out.len() / channels) as u64;
                        let drop = self.skip_frames.min(have);
                        out.drain(..drop as usize * channels);
                        self.skip_frames -= drop;
                        if out.is_empty() {
                            continue;
                        }
                    }

                    // Последний блок куска обрезаем ровно по границе,
                    // иначе в трек CUE попадёт начало следующего.
                    let frames = (out.len() / channels) as u64;
                    if let Some(limit) = self.region_frames {
                        let left = limit.saturating_sub(self.produced_frames);
                        if frames >= left {
                            out.truncate(left as usize * channels);
                            self.finished = true;
                        }
                    }
                    self.produced_frames += (out.len() / channels) as u64;
                    return Ok(!out.is_empty());
                }
                Err(SymphoniaError::DecodeError(msg)) if self.errors < MAX_CONSECUTIVE_ERRORS => {
                    self.errors += 1;
                    tracing::debug!(errors = self.errors, "пропускаю битый пакет: {msg}");
                }
                Err(err) => {
                    self.finished = true;
                    return Err(RingloftError::Decode(err.to_string()));
                }
            }
        }
    }
}

fn map_open_error(err: &SymphoniaError) -> RingloftError {
    match err {
        SymphoniaError::Unsupported(what) => RingloftError::UnsupportedFormat((*what).to_owned()),
        other => RingloftError::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Минимальный WAV: 16 бит PCM, синус 441 Гц (ровно 100 периодов в секунде
    /// при 44100) — чтобы не тянуть в тесты внешние файлы.
    fn write_wav(path: &Path, rate: u32, channels: u16, frames: usize) {
        let bytes_per_sample = 2u32;
        let data_len = frames as u32 * u32::from(channels) * bytes_per_sample;
        let mut wav = Vec::with_capacity(44 + data_len as usize);

        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * u32::from(channels) * bytes_per_sample).to_le_bytes());
        wav.extend_from_slice(&(channels * bytes_per_sample as u16).to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());

        for frame in 0..frames {
            let phase = std::f32::consts::TAU * 441.0 * frame as f32 / rate as f32;
            let value = (phase.sin() * 8000.0) as i16;
            for _ in 0..channels {
                wav.extend_from_slice(&value.to_le_bytes());
            }
        }

        std::fs::write(path, wav).expect("запись фикстуры");
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("ringloft-test-{}-{}.wav", std::process::id(), name));
        path
    }

    #[test]
    fn decodes_wav_fully() {
        let path = temp_path("decode");
        write_wav(&path, 44_100, 2, 44_100);

        let mut source = TrackSource::open(&path).expect("открытие wav");
        assert_eq!(source.spec().sample_rate, 44_100);
        assert_eq!(source.spec().channels, 2);
        assert_eq!(source.duration_ms(), Some(1000));

        let mut buffer = Vec::new();
        let mut frames = 0usize;
        while source.next_block(&mut buffer).expect("декодирование") {
            frames += buffer.len() / 2;
        }
        assert_eq!(frames, 44_100);

        // Конец потока повторяется идемпотентно.
        assert!(!source.next_block(&mut buffer).expect("конец"));
        let _ = std::fs::remove_file(&path);
    }

    /// Кусок файла должен звучать ровно от и до, без хвостов соседей.
    #[test]
    fn plays_only_the_requested_region() {
        let rate = 44_100u32;
        let path = temp_path("region");

        // Три секунды: каждая со своим уровнем.
        let mut samples = Vec::new();
        for (second, level) in [0.2_f32, 0.5, 0.8].iter().enumerate() {
            let _ = second;
            samples.extend(std::iter::repeat_n(*level, rate as usize * 2));
        }
        crate::audio::test_wav::write_wav(&path, rate, 2, &samples);

        let mut source =
            TrackSource::open_region(&path, 1000, Some(2000)).expect("кусок файла");
        assert_eq!(source.duration_ms(), Some(1000), "длительность — длина куска");

        let mut buffer = Vec::new();
        let mut frames = 0usize;
        let mut minimum = f32::MAX;
        let mut maximum = f32::MIN;
        while source.next_block(&mut buffer).expect("декодирование") {
            frames += buffer.len() / 2;
            for sample in &buffer {
                minimum = minimum.min(*sample);
                maximum = maximum.max(*sample);
            }
        }

        assert!(
            frames.abs_diff(rate as usize) < 2000,
            "ожидали около секунды, получили {frames} кадров"
        );
        assert!(
            minimum > 0.45 && maximum < 0.55,
            "в кусок попал чужой звук: уровни от {minimum:.2} до {maximum:.2}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_not_found() {
        // unwrap_err() тут не годится: TrackSource намеренно без Debug.
        match TrackSource::open(Path::new("/nope/nothing.flac")) {
            Err(err) => assert_eq!(err.code(), "NOT_FOUND"),
            Ok(_) => panic!("несуществующий файл открылся"),
        }
    }

    #[test]
    fn garbage_file_is_rejected() {
        let path = temp_path("garbage");
        std::fs::write(&path, b"this is definitely not audio").expect("запись мусора");
        assert!(TrackSource::open(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
