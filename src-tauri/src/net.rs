//! Общий HTTP для сетевых функций (AutoEQ, AcoustID, MusicBrainz, обложки):
//! один User-Agent и одинаковое отношение к 404. Ходит в сеть плеер только
//! по действию пользователя — сам по себе никуда.

use std::time::Duration;

use crate::error::{RingloftError, Result};

/// MusicBrainz требует осмысленный User-Agent с именем программы и версией.
pub const USER_AGENT: &str = concat!("Ringloft/", env!("CARGO_PKG_VERSION"), " ( desktop audio player )");

/// Тело ответа как текст. `None` — 404: «такого нет», это не ошибка.
pub fn get_text(service: &str, url: &str, timeout: Duration) -> Result<Option<String>> {
    let Some(mut response) = get(service, url, timeout)? else {
        return Ok(None);
    };
    response
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .map(Some)
        .map_err(|err| RingloftError::Network(format!("{service}: {err}")))
}

/// Тело ответа байтами (картинки). Больше `limit` не читаем.
pub fn get_bytes(service: &str, url: &str, timeout: Duration, limit: u64) -> Result<Option<Vec<u8>>> {
    let Some(mut response) = get(service, url, timeout)? else {
        return Ok(None);
    };
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map(Some)
        .map_err(|err| RingloftError::Network(format!("{service}: {err}")))
}

/// POST с полями формы (AcoustID: отпечаток длинный, в адрес его не кладут).
pub fn post_form_text(
    service: &str,
    url: &str,
    fields: &[(&str, &str)],
    timeout: Duration,
) -> Result<String> {
    let mut response = ureq::post(url)
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .send_form(fields.iter().copied())
        .map_err(|err| RingloftError::Network(format!("{service}: {err}")))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .map_err(|err| RingloftError::Network(format!("{service}: {err}")))?;
    if (200..300).contains(&status) || status == 400 {
        // 400 у AcoustID — это ответ с текстом ошибки (неверный ключ и т. п.).
        Ok(body)
    } else {
        Err(RingloftError::Network(format!("{service} ответил {status}")))
    }
}

fn get(service: &str, url: &str, timeout: Duration) -> Result<Option<ureq::http::Response<ureq::Body>>> {
    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|err| RingloftError::Network(format!("{service}: {err}")))?;
    match response.status().as_u16() {
        404 => Ok(None),
        200..=299 => Ok(Some(response)),
        status => Err(RingloftError::Network(format!("{service} ответил {status}"))),
    }
}
