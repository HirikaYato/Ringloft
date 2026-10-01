//! Отпечаток звука для AcoustID — тот же, что считает `fpcalc` (chromaprint,
//! алгоритм 2). Хватает первых двух минут: так делает и сам fpcalc, а база
//! AcoustID построена по таким же отпечаткам.

use std::path::Path;

use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};

use super::decoder::TrackSource;
use super::is_stream;
use crate::error::{RingloftError, Result};

const LENGTH_S: u64 = 120;

/// Длительность трека в секундах (её тоже просит AcoustID) и отпечаток в
/// виде строки для запроса.
pub fn fingerprint(path: &Path) -> Result<(u64, String)> {
    if is_stream(path) {
        return Err(RingloftError::Decode("у радио отпечаток не снять".to_owned()));
    }
    let mut source = TrackSource::open(path)?;
    let spec = source.spec();
    let duration_s = source.duration_ms().unwrap_or(0) / 1000;

    let config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&config);
    printer
        .start(spec.sample_rate, spec.channels as u32)
        .map_err(|err| RingloftError::Decode(format!("отпечаток: {err:?}")))?;

    let limit = LENGTH_S * u64::from(spec.sample_rate) * spec.channels as u64;
    let mut taken = 0u64;
    let mut block = Vec::new();
    let mut samples: Vec<i16> = Vec::new();
    while taken < limit && source.next_block(&mut block)? {
        // Ровно две минуты, как у fpcalc: лишний хвост блока удлинил бы отпечаток.
        let wanted = ((limit - taken) as usize).min(block.len());
        samples.clear();
        samples.extend(
            block[..wanted]
                .iter()
                .map(|value| (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16),
        );
        printer.consume(&samples);
        taken += wanted as u64;
    }
    printer.finish();

    let raw = printer.fingerprint();
    if std::env::var_os("RINGLOFT_FINGERPRINT_RAW").is_some() {
        // Для сверки с `fpcalc -raw`: тот же вид, числа через запятую.
        let text: Vec<String> = raw.iter().map(|value| (*value as i32).to_string()).collect();
        eprintln!("RAW={}", text.join(","));
    }
    let compressed = FingerprintCompressor::from(&config).compress(raw);
    if compressed.len() <= 4 {
        return Err(RingloftError::Decode("трек слишком короткий для отпечатка".to_owned()));
    }
    Ok((duration_s, base64_url(&compressed)))
}

/// base64 в варианте для адресов (`-` и `_`) и без `=` на конце — так
/// отпечатки кодирует chromaprint и так их ждёт AcoustID.
fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let symbols = chunk.len() + 1;
        for index in 0..symbols {
            let shift = 18 - 6 * index;
            out.push(char::from(ALPHABET[((value >> shift) & 0x3F) as usize]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_url_safe_base64_without_padding() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(b"foob"), "Zm9vYg");
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
    }
}
