//! Обращения к Last.fm: подпись и разбор ответов.
//!
//! Подпись у них своя: параметры сортируются по имени, склеиваются без
//! разделителей вместе со значениями, в конец дописывается секрет, и от всего
//! этого берётся md5. Пропущенная сортировка — самая частая причина
//! «Invalid method signature».

use std::time::Duration;

use md5::{Digest, Md5};
use serde::Deserialize;

use crate::error::{RingloftError, Result};

const ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct Credentials {
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
struct SessionResponse {
    session: Session,
}

#[derive(Debug, Deserialize)]
struct Session {
    name: String,
    key: String,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    error: u32,
    message: String,
}

/// Подпись запроса: md5 от склеенных по порядку пар «имя+значение» и секрета.
pub fn signature(params: &[(&str, String)], secret: &str) -> String {
    let mut sorted: Vec<&(&str, String)> = params.iter().collect();
    sorted.sort_by_key(|(name, _)| *name);

    let mut joined = String::new();
    for (name, value) in sorted {
        joined.push_str(name);
        joined.push_str(value);
    }
    joined.push_str(secret);

    let digest = Md5::digest(joined.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Запрос к API. `signed` — дописать `api_sig`; без него работают только
/// публичные методы.
pub fn call(
    credentials: &Credentials,
    method: &str,
    mut params: Vec<(&'static str, String)>,
    signed: bool,
) -> Result<String> {
    params.push(("method", method.to_owned()));
    params.push(("api_key", credentials.api_key.clone()));

    if signed {
        let api_sig = signature(&params, &credentials.api_secret);
        params.push(("api_sig", api_sig));
    }
    params.push(("format", "json".to_owned()));

    let form: Vec<(&str, &str)> = params
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();

    let mut response = ureq::post(ENDPOINT)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .send_form(form)
        .map_err(|err| RingloftError::Network(format!("Last.fm: {err}")))?;

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|err| RingloftError::Network(format!("Last.fm: {err}")))?;

    // Ошибку они отдают с кодом 200 и полем `error`, поэтому смотрим тело.
    if let Ok(failure) = serde_json::from_str::<ApiError>(&body) {
        return Err(RingloftError::Network(format!(
            "Last.fm {}: {}",
            failure.error, failure.message
        )));
    }
    Ok(body)
}

/// Первый шаг входа: одноразовый токен, который пользователь подтверждает
/// в браузере.
pub fn request_token(credentials: &Credentials) -> Result<String> {
    let body = call(credentials, "auth.getToken", Vec::new(), true)?;
    let parsed: TokenResponse = serde_json::from_str(&body)
        .map_err(|err| RingloftError::Network(format!("Last.fm: непонятный ответ ({err})")))?;
    Ok(parsed.token)
}

/// Страница, на которой пользователь разрешает доступ.
pub fn auth_url(credentials: &Credentials, token: &str) -> String {
    format!(
        "https://www.last.fm/api/auth/?api_key={}&token={token}",
        credentials.api_key
    )
}

/// Второй шаг: обмен подтверждённого токена на постоянный ключ сессии.
pub fn request_session(credentials: &Credentials, token: &str) -> Result<(String, String)> {
    let body = call(
        credentials,
        "auth.getSession",
        vec![("token", token.to_owned())],
        true,
    )?;
    let parsed: SessionResponse = serde_json::from_str(&body)
        .map_err(|err| RingloftError::Network(format!("Last.fm: непонятный ответ ({err})")))?;
    Ok((parsed.session.name, parsed.session.key))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пример из документации Last.fm: параметры сортируются по имени, потом
    /// склеиваются с секретом. Без сортировки подпись не сходится.
    #[test]
    fn signs_in_alphabetical_order() {
        let params = vec![
            ("method", "auth.getSession".to_owned()),
            ("api_key", "xxx".to_owned()),
            ("token", "yyy".to_owned()),
        ];
        let signed = signature(&params, "secret");

        let expected = {
            let joined = "api_keyxxxmethodauth.getSessiontokenyyysecret";
            let digest = Md5::digest(joined.as_bytes());
            digest.iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        assert_eq!(signed, expected);
        assert_eq!(signed.len(), 32);
    }
}
