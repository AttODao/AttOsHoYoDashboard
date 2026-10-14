//! Cookie parsing and normalization, without ownership verification.
use super::{Credentials, MAX_COOKIE_LEN};
use crate::dashboard::error::{Error, Result};
use std::collections::HashMap;

#[cfg(test)]
mod tests;

impl Credentials {
    pub(in crate::dashboard) fn parse(raw: &str) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_COOKIE_LEN {
            return Err(Error::Invalid("Cookie header must contain 1 to 8192 bytes"));
        }
        if !raw.is_ascii()
            || raw
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == 0x7f)
        {
            return Err(Error::Invalid("Cookie header must be printable ASCII"));
        }

        let pairs = parse_pairs(raw)?;
        let has_legacy_token = pairs.contains_key("ltoken");
        let has_v2_token = pairs.contains_key("ltoken_v2");
        if has_legacy_token && has_v2_token {
            return Err(Error::Invalid(
                "Cookie header mixes HoYoLAB credential families",
            ));
        }
        if has_legacy_token {
            return build_legacy(&pairs);
        }
        if has_v2_token {
            return build_v2(&pairs);
        }
        Err(Error::Invalid(
            "Cookie header is missing HoYoLAB credentials",
        ))
    }
}

fn parse_pairs(raw: &str) -> Result<HashMap<&str, &str>> {
    let mut pairs = HashMap::new();
    for part in raw.split(';') {
        let part = part.trim_matches(' ');
        let (name, value) = part
            .split_once('=')
            .ok_or(Error::Invalid("Cookie header contains a malformed pair"))?;
        if name.is_empty() || !name.bytes().all(is_cookie_name_byte) {
            return Err(Error::Invalid("Cookie header contains an invalid name"));
        }
        if pairs.insert(name, value).is_some() {
            return Err(Error::Invalid("Cookie header contains duplicate names"));
        }
    }
    Ok(pairs)
}

fn build_legacy(pairs: &HashMap<&str, &str>) -> Result<Credentials> {
    let token = parse_cookie_token(pairs.get("ltoken").copied().ok_or(Error::Invalid(
        "Cookie header is missing HoYoLAB credentials",
    ))?)?;
    let claimed_uid = resolve_uid_aliases(pairs, &["ltuid", "account_id"])?;
    reject_conflicting_uid_aliases(pairs, &claimed_uid, &["ltuid", "account_id"])?;
    let header = format!("ltoken={token}; ltuid={claimed_uid}");
    Ok(Credentials {
        header,
        claimed_uid,
    })
}

fn build_v2(pairs: &HashMap<&str, &str>) -> Result<Credentials> {
    let token = parse_cookie_token(pairs.get("ltoken_v2").copied().ok_or(Error::Invalid(
        "Cookie header is missing HoYoLAB credentials",
    ))?)?;
    let claimed_uid = resolve_uid_aliases(pairs, &["ltuid_v2", "account_id_v2"])?;
    reject_conflicting_uid_aliases(pairs, &claimed_uid, &["ltuid_v2", "account_id_v2"])?;
    let mid = resolve_optional_token_aliases(pairs, &["ltmid_v2", "account_mid_v2"])?;

    let mut header = format!("ltoken_v2={token}; ltuid_v2={claimed_uid}");
    if let Some(mid) = mid {
        header.push_str("; ltmid_v2=");
        header.push_str(&mid);
    }
    Ok(Credentials {
        header,
        claimed_uid,
    })
}

fn resolve_uid_aliases(pairs: &HashMap<&str, &str>, aliases: &[&str]) -> Result<String> {
    let mut uid = None;
    for alias in aliases {
        if let Some(value) = pairs.get(alias).copied() {
            let parsed = parse_uid(value)?;
            if uid.as_deref().is_some_and(|uid| uid != parsed) {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting UID aliases",
                ));
            }
            uid = Some(parsed);
        }
    }
    uid.ok_or(Error::Invalid("Cookie header is missing HoYoLAB UID"))
}

fn reject_conflicting_uid_aliases(
    pairs: &HashMap<&str, &str>,
    selected_uid: &str,
    selected_aliases: &[&str],
) -> Result<()> {
    for alias in [
        "ltuid",
        "account_id",
        "ltuid_v2",
        "account_id_v2",
        "stuid",
        "stuid_v2",
    ] {
        if selected_aliases.contains(&alias) {
            continue;
        }
        if let Some(value) = pairs.get(alias).copied() {
            let parsed = parse_uid(value)?;
            if parsed != selected_uid {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting UID aliases",
                ));
            }
        }
    }
    Ok(())
}

fn resolve_optional_token_aliases(
    pairs: &HashMap<&str, &str>,
    aliases: &[&str],
) -> Result<Option<String>> {
    let mut token = None;
    for alias in aliases {
        if let Some(value) = pairs.get(alias).copied() {
            let parsed = parse_cookie_token(value)?.to_owned();
            if token.as_deref().is_some_and(|token| token != parsed) {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting MID aliases",
                ));
            }
            token = Some(parsed);
        }
    }
    Ok(token)
}

fn parse_uid(value: &str) -> Result<String> {
    reject_placeholder(value)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::Invalid(
            "HoYoLAB UID must be a positive decimal integer",
        ));
    }
    let uid: u64 = value
        .parse()
        .map_err(|_| Error::Invalid("HoYoLAB UID must be a positive decimal integer"))?;
    if uid == 0 {
        return Err(Error::Invalid(
            "HoYoLAB UID must be a positive decimal integer",
        ));
    }
    Ok(uid.to_string())
}

fn parse_cookie_token(value: &str) -> Result<&str> {
    reject_placeholder(value)?;
    if value.is_empty() || !value.bytes().all(is_cookie_value_token_byte) {
        return Err(Error::Invalid(
            "HoYoLAB Cookie token contains invalid characters",
        ));
    }
    Ok(value)
}

fn reject_placeholder(value: &str) -> Result<()> {
    if value.starts_with("YOUR_") || value.contains("...") {
        return Err(Error::Invalid(
            "HoYoLAB Cookie contains a placeholder value",
        ));
    }
    Ok(())
}

fn is_cookie_name_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
            | b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
    )
}

fn is_cookie_value_token_byte(byte: u8) -> bool {
    byte.is_ascii_graphic() && !matches!(byte, b';' | b'"' | b',' | b'\\')
}
