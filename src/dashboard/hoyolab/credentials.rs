//! HoYoLAB credential interface; Cookie claims are not ownership proof.
mod crypto;
mod parser;

const MAX_COOKIE_LEN: usize = 8192;
const KEY_LEN: usize = 32;

pub(in crate::dashboard) struct Credentials {
    header: String,
    claimed_uid: String,
}

impl Credentials {
    pub(in crate::dashboard) fn as_str(&self) -> &str {
        &self.header
    }

    // This is only the UID claimed inside the Cookie. It is not ownership proof.
    pub(in crate::dashboard) fn claimed_uid(&self) -> &str {
        &self.claimed_uid
    }
}

pub(in crate::dashboard) struct CookieKey([u8; KEY_LEN]);
