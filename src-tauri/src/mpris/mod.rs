//! MPRIS: системный виджет проигрывателя и медиаклавиши на Linux.
//!
//! Написано своими руками на zbus вместо готового крейта намеренно.
//! Рабочие столы решают, какому плееру отдать медиаклавишу, по данным с
//! шины, поэтому интерфейс должен соответствовать спецификации дословно —
//! включая `DesktopEntry` и `HasTrackList` (а не `HasTracklist`, как его
//! пишет souvlaki), живую `Position` и сигнал `Seeked`.

mod player;
mod root;

use crossbeam_channel::Receiver;
use zbus::Connection;
use zbus::object_server::InterfaceRef;

use crate::media_controls::{MediaContext, MediaUpdate};
use player::Player;
use root::Root;

/// `true` — поток поднялся. Плеер обязан работать и без шины.
pub fn spawn(ctx: MediaContext, rx: Receiver<MediaUpdate>) -> bool {
    let spawned = std::thread::Builder::new()
        .name("ringloft-mpris".into())
        .spawn(move || run(ctx, rx));

    match spawned {
        Ok(_) => true,
        Err(err) => {
            tracing::error!(%err, "не удалось запустить поток MPRIS");
            false
        }
    }
}

fn run(ctx: MediaContext, rx: Receiver<MediaUpdate>) {
    let connection = match zbus::block_on(serve(ctx)) {
        Ok(connection) => connection,
        Err(err) => {
            tracing::warn!(%err, "MPRIS недоступен");
            return;
        }
    };
    tracing::info!("медиаклавиши подключены");

    // Ждём на обычном канале: соединение zbus живёт своими задачами и в
    // опросе не нуждается.
    while let Ok(update) = rx.recv() {
        if let Err(err) = zbus::block_on(apply(&connection, update)) {
            tracing::debug!(%err, "MPRIS не принял обновление");
        }
    }
}

async fn serve(ctx: MediaContext) -> zbus::Result<Connection> {
    zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(PATH, Root::new(ctx.clone()))?
        .serve_at(PATH, Player::new(ctx))?
        .build()
        .await
}

async fn apply(connection: &Connection, update: MediaUpdate) -> zbus::Result<()> {
    let iface: InterfaceRef<Player> = connection.object_server().interface(PATH).await?;
    match update {
        MediaUpdate::Track(meta) => {
            {
                let mut player = iface.get_mut().await;
                player.track_id = player.track_id.wrapping_add(1);
                player.meta = meta;
            }
            iface.get().await.metadata_changed(iface.signal_emitter()).await?;
        }
        MediaUpdate::Cover(cover_url) => {
            iface.get_mut().await.meta.cover_url = cover_url;
            iface.get().await.metadata_changed(iface.signal_emitter()).await?;
        }
        MediaUpdate::Playback {
            status,
            position_ms,
        } => {
            // Позиция меняется постоянно, и по спецификации сигналом её не
            // рассылают: клиент либо спросит сам, либо получит `Seeked`.
            let changed = {
                let mut player = iface.get_mut().await;
                player.position_us = position_ms as i64 * 1_000;
                let changed = player.status != status;
                player.status = status;
                changed
            };
            if changed {
                iface
                    .get()
                    .await
                    .playback_status_changed(iface.signal_emitter())
                    .await?;
            }
        }
    }
    Ok(())
}


/// Имена интерфейсов и объекта видны обеим половинам модуля.
const PATH: &str = "/org/mpris/MediaPlayer2";
const BUS_NAME: &str = "org.mpris.MediaPlayer2.ringloft";
