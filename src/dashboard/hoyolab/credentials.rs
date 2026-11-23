//! HoYoLAB credential interface; Cookie claims are not ownership proof.
mod crypto;
mod parser;

const MAX_COOKIE_LEN: usize = 8192;
const KEY_LEN: usize = 32;

pub(in crate::dashboard) struct Credentials {
    /// The ltoken pair (and MID) every read and the daily check-in use.
    header: String,
    claimed_uid: String,
    /// `cookie_token_v2=…; account_id_v2=…[; account_mid_v2=…]`, sent only to the code
    /// redemption endpoints; absent for Cookies registered without it.
    redemption: Option<String>,
}

impl Credentials {
    pub(in crate::dashboard) fn as_str(&self) -> &str {
        &self.header
    }

    // This is only the UID claimed inside the Cookie. It is not ownership proof.
    pub(in crate::dashboard) fn claimed_uid(&self) -> &str {
        &self.claimed_uid
    }

    /// The Cookie header for code redemption, when the Cookie carries it.
    pub(in crate::dashboard) fn redemption(&self) -> Option<&str> {
        self.redemption.as_deref()
    }

    /// What is sealed: the ltoken header, then the redemption credentials if any. Parsing
    /// it again yields the same credentials, so older blobs without them still open.
    fn sealed_text(&self) -> String {
        match &self.redemption {
            Some(redemption) => format!("{}; {redemption}", self.header),
            None => self.header.clone(),
        }
    }
}

pub(in crate::dashboard) struct CookieKey([u8; KEY_LEN]);
