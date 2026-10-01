//! Кадры Discord IPC. Протокол простой: 4 байта опкода, 4 байта длины (оба
//! little-endian) и JSON. Готовый крейт здесь не нужен — это сотня строк, а
//! поведение при отсутствующем Discord важнее, чем удобство API.

use std::io::{Read, Write};
use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

use crate::error::{RingloftError, Result};

/// Рукопожатие.
const OP_HANDSHAKE: u32 = 0;
/// Обычный кадр с командой.
const OP_FRAME: u32 = 1;
/// Ответ Discord читаем, но не разбираем: важно его вычитать, иначе буфер
/// сокета переполнится и следующая запись встанет.
const REPLY_BUFFER: usize = 4096;
/// Таймаут есть только у Unix-сокета: у именованной трубы Windows его
/// выставить нечем, а Discord отвечает на каждый кадр сам.
#[cfg(unix)]
const TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(unix)]
type Socket = std::os::unix::net::UnixStream;
#[cfg(windows)]
type Socket = std::fs::File;

pub struct Ipc {
    socket: Socket,
}

impl Ipc {
    /// Подключается к запущенному Discord и здоровается. Ошибка здесь —
    /// обычное дело: Discord может быть просто не запущен.
    pub fn connect(client_id: &str) -> Result<Self> {
        let mut last: Option<String> = None;
        for path in candidates() {
            match Self::connect_at(&path, client_id) {
                Ok(ipc) => {
                    tracing::debug!(path = %path.display(), "Discord: подключились");
                    return Ok(ipc);
                }
                Err(err) => last = Some(err.to_string()),
            }
        }
        Err(RingloftError::Internal(format!(
            "Discord не отвечает: {}",
            last.unwrap_or_else(|| "сокет не найден".to_owned())
        )))
    }

    /// Подключение к конкретному сокету. Отдельно от `connect` ради тестов:
    /// протокол проверяется на своём сокете, без запущенного Discord.
    pub fn connect_at(path: &std::path::Path, client_id: &str) -> Result<Self> {
        let socket = open(path).map_err(RingloftError::Internal)?;
        let mut ipc = Self { socket };
        ipc.handshake(client_id)?;
        Ok(ipc)
    }

    fn handshake(&mut self, client_id: &str) -> Result<()> {
        let payload = serde_json::json!({ "v": 1, "client_id": client_id });
        self.write_frame(OP_HANDSHAKE, &payload.to_string())?;
        check(self.read_reply())
    }

    /// Ставит или снимает активность. `None` — снять.
    pub fn set_activity(&mut self, activity: Option<serde_json::Value>) -> Result<()> {
        let payload = serde_json::json!({
            "cmd": "SET_ACTIVITY",
            "nonce": nonce(),
            "args": { "pid": std::process::id(), "activity": activity },
        });
        self.write_frame(OP_FRAME, &payload.to_string())?;
        check(self.read_reply())
    }

    fn write_frame(&mut self, op: u32, payload: &str) -> Result<()> {
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload.as_bytes());

        self.socket
            .write_all(&frame)
            .map_err(|err| RingloftError::Internal(format!("Discord: запись не удалась: {err}")))
    }

    /// Вычитывает ответ. Читать обязательно, даже если он не нужен: Discord
    /// отвечает на каждый кадр, и непрочитанное переполнит буфер сокета.
    /// Молчание ответом не считаем — это не повод останавливать плеер.
    fn read_reply(&mut self) -> Option<serde_json::Value> {
        let mut header = [0u8; 8];
        self.socket.read_exact(&mut header).ok()?;
        let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let mut rest = vec![0u8; length.min(REPLY_BUFFER)];
        self.socket.read_exact(&mut rest).ok()?;
        serde_json::from_slice(&rest).ok()
    }
}

/// Discord сообщает об ошибке обычным кадром: неверный идентификатор
/// приложения выглядит как успешная запись, если ответ не смотреть.
fn check(reply: Option<serde_json::Value>) -> Result<()> {
    let Some(reply) = reply else {
        return Ok(());
    };
    if reply.get("evt").and_then(|evt| evt.as_str()) != Some("ERROR") {
        return Ok(());
    }
    let message = reply
        .get("data")
        .and_then(|data| data.get("message"))
        .and_then(|message| message.as_str())
        .unwrap_or("Discord отказал");
    Err(RingloftError::Internal(message.to_owned()))
}

#[cfg(unix)]
fn open(path: &std::path::Path) -> std::result::Result<Socket, String> {
    let socket = std::os::unix::net::UnixStream::connect(path).map_err(|err| err.to_string())?;
    // Без таймаутов поток навсегда встанет на чтении ответа, если Discord
    // закрылся между кадрами.
    let _ = socket.set_read_timeout(Some(TIMEOUT));
    let _ = socket.set_write_timeout(Some(TIMEOUT));
    Ok(socket)
}

#[cfg(windows)]
fn open(path: &std::path::Path) -> std::result::Result<Socket, String> {
    // На Windows это именованный труба, и открывается она обычным файловым API.
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|err| err.to_string())
}

/// Где искать сокет. Discord нумерует их от 0 до 9, а во flatpak и snap он
/// лежит в своей подпапке.
#[cfg(unix)]
fn candidates() -> Vec<PathBuf> {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    let prefixes = [
        "",
        "app/com.discordapp.Discord/",
        "app/com.discordapp.DiscordCanary/",
        "snap.discord/",
        ".flatpak/dev.vencord.Vesktop/xdg-run/",
    ];

    let mut paths = Vec::with_capacity(prefixes.len() * 10);
    for prefix in prefixes {
        for index in 0..10 {
            paths.push(base.join(format!("{prefix}discord-ipc-{index}")));
        }
    }
    paths
}

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
    (0..10)
        .map(|index| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{index}")))
        .collect()
}

/// Discord требует уникальный nonce на кадр; время подходит.
fn nonce() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::net::UnixListener;

    /// Читает один кадр: опкод, длина, JSON.
    fn read_frame(stream: &mut std::os::unix::net::UnixStream) -> (u32, serde_json::Value) {
        let mut header = [0u8; 8];
        stream.read_exact(&mut header).expect("заголовок");
        let op = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let mut payload = vec![0u8; length];
        stream.read_exact(&mut payload).expect("тело");
        (op, serde_json::from_slice(&payload).expect("json"))
    }

    /// Свой сокет вместо Discord: проверяем именно кадры, которые уходят.
    #[test]
    fn frames_match_the_protocol() {
        let path = std::env::temp_dir().join(format!("ringloft-discord-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("сокет");

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("соединение");
            let mut frames = Vec::new();
            for _ in 0..3 {
                let frame = read_frame(&mut stream);
                // Discord отвечает на каждый кадр — отвечаем и мы, иначе
                // клиент будет ждать таймаут на чтении.
                let reply = br#"{"cmd":"DISPATCH","evt":"READY"}"#;
                let mut out = Vec::new();
                out.extend_from_slice(&1u32.to_le_bytes());
                out.extend_from_slice(&(reply.len() as u32).to_le_bytes());
                out.extend_from_slice(reply);
                let _ = std::io::Write::write_all(&mut stream, &out);
                frames.push(frame);
            }
            frames
        });

        let mut ipc = Ipc::connect_at(&path, "123456789").expect("подключение");
        ipc.set_activity(Some(serde_json::json!({ "details": "Панелька" })))
            .expect("активность");
        ipc.set_activity(None).expect("снятие активности");

        let frames = server.join().expect("сервер");
        let _ = std::fs::remove_file(&path);

        // Рукопожатие.
        assert_eq!(frames[0].0, OP_HANDSHAKE);
        assert_eq!(frames[0].1["v"], 1);
        assert_eq!(frames[0].1["client_id"], "123456789");

        // Активность.
        assert_eq!(frames[1].0, OP_FRAME);
        assert_eq!(frames[1].1["cmd"], "SET_ACTIVITY");
        assert_eq!(frames[1].1["args"]["activity"]["details"], "Панелька");
        assert_eq!(
            frames[1].1["args"]["pid"].as_u64(),
            Some(u64::from(std::process::id()))
        );
        assert!(frames[1].1["nonce"].is_string());

        // Снятие: активность null, а не отсутствующее поле.
        assert!(frames[2].1["args"]["activity"].is_null());
        assert_ne!(frames[1].1["nonce"], frames[2].1["nonce"], "nonce уникален");
    }
}
