//! Объект `org.mpris.MediaPlayer2` — «кто мы такие».
//!
//! `DesktopEntry` здесь не для красоты: по нему рабочий стол связывает
//! плеер на шине с установленным приложением.

use crate::media_controls::{MediaAction, MediaContext, handle_action};

/// Имя `.desktop`-файла без расширения.
const DESKTOP_ENTRY: &str = "ringloft";

pub(super) struct Root {
    ctx: MediaContext,
}

impl Root {
    pub(super) fn new(ctx: MediaContext) -> Self {
        Self { ctx }
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        handle_action(&self.ctx, MediaAction::Raise);
    }

    fn quit(&self) {
        handle_action(&self.ctx, MediaAction::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        "Ringloft"
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        DESKTOP_ENTRY
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        // http и https — интернет-радио: OpenUri играет и его.
        ["file", "http", "https"]
            .iter()
            .map(|scheme| (*scheme).to_owned())
            .collect()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        [
            "audio/mpeg",
            "audio/flac",
            "audio/x-flac",
            "audio/ogg",
            "audio/x-vorbis+ogg",
            "audio/x-opus+ogg",
            "audio/opus",
            "audio/x-wav",
            "audio/mp4",
            "audio/x-m4a",
            "audio/aac",
            "audio/x-aiff",
        ]
            .iter()
            .map(|mime| (*mime).to_owned())
            .collect()
    }
}
