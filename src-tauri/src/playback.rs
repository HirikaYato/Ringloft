//! Действия воспроизведения, общие для команд Tauri, медиаклавиш и трея.
//!
//! Раньше «включить следующий» жило в трёх местах; теперь логика одна, а
//! вызывающие отличаются только тем, откуда берут ссылки.

use std::path::PathBuf;
use std::sync::Arc;

use crate::audio::{AudioCmd, EngineHandle};
use crate::error::Result;
use crate::playlist::{PlaybackMode, PlaylistStore, is_list_path};
use crate::settings::{RepeatMode, Settings, SettingsStore};

pub struct Controls<'a> {
    pub engine: &'a EngineHandle,
    pub playlist: &'a Arc<PlaylistStore>,
    pub settings: &'a Arc<SettingsStore>,
}

impl Controls<'_> {
    /// `manual = true` — нажали кнопку: повтор одного трека игнорируется.
    pub fn next(&self, manual: bool) -> Result<()> {
        let settings = self.settings.get();
        match self
            .playlist
            .advance(settings.playback.repeat, settings.playback.shuffle, manual)
        {
            Some(track) => self.engine.send(AudioCmd::Load {
                track,
                autoplay: true,
            }),
            None => {
                tracing::debug!("плейлист кончился");
                Ok(())
            }
        }
    }

    pub fn prev(&self) -> Result<()> {
        let settings = self.settings.get();
        match self.playlist.go_back(settings.playback.shuffle) {
            Some(track) => self.engine.send(AudioCmd::Load {
                track,
                autoplay: true,
            }),
            None => Ok(()),
        }
    }

    pub fn play_index(&self, index: usize) -> Result<()> {
        match self.playlist.id_at(index) {
            Some(id) => self.play_item(id),
            None => Ok(()),
        }
    }

    /// Играет строку из любой вкладки — та становится играющей и, если ещё
    /// не играла, забирает текущий режим повтора и перемешивания.
    pub fn play_item(&self, id: u64) -> Result<()> {
        let was_playing = self.playlist.playing_tab_id();
        let Some(track) = self.playlist.play_item(id) else {
            return Ok(());
        };
        if self.playlist.playing_tab_id() != was_playing {
            // Перешли слушать другую вкладку — это новый круг перемешивания,
            // и начинаться он должен с выбранного трека. Иначе выбранный
            // оказывался посреди старого круга, и треки «до» него в этом
            // круге уже не звучали.
            self.playlist.restart_shuffle();
            self.sync_mode()?;
        }
        self.engine.send(AudioCmd::Load {
            track,
            autoplay: true,
        })
    }

    /// Пропустить трек, который не открылся. Это естественный переход, как
    /// в конце трека: список без повтора на последнем треке кончается, с
    /// повтором — идёт по кругу. Не в счёт только повтор одного трека — иначе
    /// плеер снова и снова открывал бы тот же битый файл.
    ///
    /// Кнопка «вперёд» (`next(true)`) тут не годится: она и в конце круга
    /// перемешивания идёт дальше, то есть перетасовывает и начинает заново.
    pub fn skip_broken(&self) -> Result<()> {
        let settings = self.settings.get();
        let repeat = match settings.playback.repeat {
            RepeatMode::Track => RepeatMode::Off,
            other => other,
        };
        match self
            .playlist
            .advance(repeat, settings.playback.shuffle, false)
        {
            Some(track) => self.engine.send(AudioCmd::Load {
                track,
                autoplay: true,
            }),
            None => {
                tracing::debug!("после пропуска играть нечего: список кончился");
                Ok(())
            }
        }
    }

    /// Открыть пути «снаружи»: двойным щелчком в проводнике, второй копией
    /// приложения или через MPRIS. `true` — что-то заиграло.
    ///
    /// Плейлисты (m3u, cue) открываются каждый в своей вкладке. Остальное —
    /// во вкладку «Открытые файлы»: в активный список такие файлы **не
    /// попадают**, открыть трек значит послушать его, а не дописать в
    /// собранный руками плейлист. Несколько файлов — это очередь из них.
    pub fn open_paths(&self, paths: &[PathBuf]) -> Result<bool> {
        let (lists, files): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.iter().cloned().partition(|path| is_list_path(path));

        let mut first_of_list = None;
        for list in &lists {
            let is_cue = list
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("cue"));
            let result = if is_cue {
                self.playlist.import_cue(list)
            } else {
                self.playlist.import_m3u(list)
            };
            match result {
                Ok((added, listed)) => {
                    tracing::info!(added, listed, path = %list.display(), "список открыт");
                    // Импорт делает новую вкладку открытой: её первая строка
                    // и есть начало списка.
                    if first_of_list.is_none() && added > 0 {
                        first_of_list = self.playlist.id_at(0);
                    }
                }
                Err(err) => tracing::error!(%err, path = %list.display(), "список не открылся"),
            }
        }

        // Отдельные файлы важнее списков: их открывали ради прослушивания.
        let target = match self.playlist.open_external(&files) {
            Some(id) => Some(id),
            None => first_of_list,
        };
        tracing::info!(files = files.len(), lists = lists.len(), "открыты пути снаружи");

        match target {
            Some(id) => {
                self.play_item(id)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Повтор и перемешивание меняются сразу в двух местах: в настройках
    /// (их читает плеер) и в памяти играющего списка — чтобы при возврате к
    /// нему режим восстановился. Единственный путь их поменять.
    pub fn set_mode(&self, repeat: RepeatMode, shuffle: bool) -> Result<Settings> {
        let settings = self.settings.patch(serde_json::json!({
            "playback": { "repeat": repeat, "shuffle": shuffle }
        }))?;
        self.playlist.remember_mode(PlaybackMode {
            repeat: settings.playback.repeat,
            shuffle: settings.playback.shuffle,
        });
        Ok(settings)
    }

    /// Каждый список помнит, как его слушали: переключились на него — вернулся
    /// и его режим. Список, который ещё ни разу не играл, забирает текущий.
    fn sync_mode(&self) -> Result<()> {
        match self.playlist.playing_mode() {
            Some(mode) => {
                self.settings.patch(serde_json::json!({
                    "playback": { "repeat": mode.repeat, "shuffle": mode.shuffle }
                }))?;
            }
            None => {
                let playback = self.settings.get().playback;
                self.playlist.remember_mode(PlaybackMode {
                    repeat: playback.repeat,
                    shuffle: playback.shuffle,
                });
            }
        }
        Ok(())
    }

    /// Перемотка на `delta_ms` от текущей позиции, с упором в границы трека.
    pub fn seek_relative(&self, delta_ms: i64) -> Result<()> {
        let state = self.engine.state();
        let position = state.position_ms() as i64;
        let duration = state.duration_ms() as i64;
        let mut target = position.saturating_add(delta_ms).max(0);
        if duration > 0 {
            target = target.min(duration);
        }
        self.engine.send(AudioCmd::Seek(target as u64))
    }

    pub fn set_volume(&self, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.0);
        self.engine.send(AudioCmd::SetVolume(volume))?;
        self.settings
            .patch(serde_json::json!({ "playback": { "volume": volume } }))?;
        Ok(())
    }
}
