//! Protocol errors contain classifications, never raw requests or upstream messages.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidInput(String),
    #[error("HTTPクライアントの作成に失敗しました")]
    ClientConfiguration,
    #[error("HTTPリクエストに失敗しました")]
    Network,
    #[error("HTTP {0}")]
    Http(u16),
    #[error("レスポンス読み取りに失敗しました")]
    ResponseRead,
    #[error("レスポンスが上限{0} bytesを超えました")]
    ResponseTooLarge(usize),
    #[error("JSONレスポンスの形式が不正です")]
    InvalidEnvelope,
    #[error("HoYoLAB retcode={code}{hint}", hint = retcode_hint(*code))]
    Upstream { code: i64 },
    #[error("HoYoLAB APIがdataを返しませんでした")]
    MissingData,
    #[error("{0}")]
    InvalidData(String),
    #[error("システム時刻が不正です")]
    Clock,
}

pub type Result<T> = std::result::Result<T, Error>;

// Input validation and CLI diagnostics already redact their secret-bearing values.
impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::InvalidInput(message.to_owned())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::InvalidInput(message)
    }
}

// Existing callers with a String error boundary can continue to use `?`.
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}

fn retcode_hint(code: i64) -> &'static str {
    // Hints from current observations, not guarantees or retry/ownership policy.
    match code {
        -100 | 10001 | -1071 => "（Cookieが無効または期限切れです）",
        10101 => "（ゲームレコードの1日あたり参照上限の可能性があります）",
        10102 => "（HoYoLABで戦績またはリアルタイム状態の公開を有効にしてください）",
        _ => "",
    }
}
