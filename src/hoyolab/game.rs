use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::models::{daily, genshin, starrail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Genshin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Starrail;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum GameKind {
    Genshin(Genshin),
    Starrail(Starrail),
}

impl TryFrom<String> for GameKind {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "genshin" => Ok(Self::Genshin(Genshin)),
            "starrail" => Ok(Self::Starrail(Starrail)),
            _ => Err("game must be genshin or starrail"),
        }
    }
}

impl From<GameKind> for String {
    fn from(game: GameKind) -> Self {
        match game {
            GameKind::Genshin(_) => "genshin",
            GameKind::Starrail(_) => "starrail",
        }
        .into()
    }
}

pub trait Game {
    const KIND: GameKind;
    const RECORD_BASE: &'static str;
    const NOTE_PATH: &'static str;
    const DAILY_BASE: &'static str;
    const ACT_ID: &'static str;
    const SIGN_GAME: &'static str;

    type Index: DeserializeOwned + Serialize;
    type DailyNote: DeserializeOwned + Serialize;
    type DailyInfo: DeserializeOwned + Serialize;
    type DailySign: DeserializeOwned + Serialize;
    type DailyHome: DeserializeOwned + Serialize;
}

impl Game for Genshin {
    const KIND: GameKind = GameKind::Genshin(Genshin);
    const RECORD_BASE: &'static str =
        "https://sg-public-api.hoyolab.com/event/game_record/genshin/api";
    const NOTE_PATH: &'static str = "dailyNote";
    const DAILY_BASE: &'static str = "https://sg-hk4e-api.hoyolab.com/event/sol";
    const ACT_ID: &'static str = "e202102251931481";
    const SIGN_GAME: &'static str = "hk4e";

    type Index = genshin::Index;
    type DailyNote = genshin::DailyNote;
    type DailyInfo = genshin::DailyInfo;
    type DailySign = daily::DailySign;
    type DailyHome = daily::DailyHome;
}

impl Game for Starrail {
    const KIND: GameKind = GameKind::Starrail(Starrail);
    const RECORD_BASE: &'static str = "https://bbs-api-os.hoyolab.com/game_record/hkrpg/api";
    const NOTE_PATH: &'static str = "note";
    const DAILY_BASE: &'static str = "https://sg-public-api.hoyolab.com/event/luna/hkrpg/os";
    const ACT_ID: &'static str = "e202303301540311";
    const SIGN_GAME: &'static str = "hkrpg";

    type Index = starrail::Index;
    type DailyNote = starrail::DailyNote;
    type DailyInfo = starrail::DailyInfo;
    type DailySign = daily::DailySign;
    type DailyHome = daily::DailyHome;
}
