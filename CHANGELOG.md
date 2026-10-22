# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versions follow [Semantic Versioning](https://semver.org/). Each version's
section becomes the description of its GitHub release.

## [0.2.0] — 2026-10-22

The phone version.

### Android
- Ringloft for Android 8.0 and newer: one APK for 64-bit and 32-bit ARM
  phones, published on the GitHub releases page. Music is played straight
  from the phone's folders, which is why the app asks for *All files access*.
- Plays in the background and with the screen off: a player card in the
  notification shade and on the lock screen, headset and Bluetooth buttons,
  pauses for calls and when headphones are unplugged.
- An interface made for the phone: playlists, a library with albums and
  artists, and a full-screen player with lyrics, the play queue, a sleep
  timer and the equalizer. Swipe the cover to change tracks, swipe down to
  close the player.
- The same playlists and library features as on the desktop: folders, smart
  lists, listening stats, Last.fm, internet radio.
- Not on the phone yet: tag editing, file renaming, the visualizer, batch
  jobs (loudness, lyrics, covers), playlist export and Discord.

### Fixed
- Replaying the track that was already playing broke the gapless
  transition to the next one.
- A brief glitch on the audio device side silenced playback until the next
  track while the status still said "playing".
- An unreachable library folder (an unplugged drive) no longer wipes its
  tracks together with play counts and ratings, and an empty playlist source
  folder no longer empties the playlist.

## [0.1.0] — 2026-10-02

First release.

### Audio
- FLAC, MP3, AAC/M4A, ALAC, Ogg Vorbis, Opus, WAV, AIFF, MKA; CUE images;
  internet radio with titles from the stream.
- Gapless playback and crossfade, ReplayGain from tags and built-in loudness
  analysis (EBU R128) that writes the tags into the files.
- A 10-band equalizer and AutoEQ headphone profiles.
- Playback at the file's sample rate without resampling (via PipeWire, once
  allowed in the settings).
- A sleep timer and "stop after this track" with a smooth fade-out.

### Playlists and library
- Playlists as tabs or a sidebar, reordered by dragging; a play queue;
  repeat and shuffle remembered per playlist.
- Playlists remember their folders and update from them.
- Library: albums, artists, genres, folders, smart lists, search, listening
  stats, duplicate and missing file detection.
- Configurable playlist columns.

### Tags, lyrics, covers
- Tag editing, renaming by template, deleting to the trash.
- Tags by audio fingerprint (AcoustID and MusicBrainz), covers from iTunes
  and the Cover Art Archive, written into the files.
- Synced lyrics from `.lrc` files, tags and lrclib.net.

### System
- MPRIS (Linux), system media controls (Windows), media keys, tray icon,
  notifications, command-line control.
- Last.fm, Discord Rich Presence.
- Themes, a custom accent color or one taken from the cover, font and font
  size choice.
