//! Endgame challenge record endpoints (GET only). Their payloads stay untyped JSON:
//! the field names come from genshin.py and have not been checked against live
//! responses, so callers read them defensively.
use super::game::{Game, GameKind, Genshin, Starrail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Challenge {
    /// Genshin Spiral Abyss, `GET /spiralAbyss`.
    SpiralAbyss,
    /// Genshin Imaginarium Theater, `GET /role_combat`.
    ImaginariumTheater,
    /// Genshin Stygian Onslaught, `GET /hard_challenge`.
    StygianOnslaught,
    /// Star Rail Memory of Chaos, `GET /challenge`.
    MemoryOfChaos,
    /// Star Rail Pure Fiction, `GET /challenge_story`.
    PureFiction,
    /// Star Rail Apocalyptic Shadow, `GET /challenge_boss`.
    ApocalypticShadow,
    /// Star Rail Anomaly Arbitration, `GET /challenge_peak`.
    AnomalyArbitration,
}

const GENSHIN: [Challenge; 3] = [
    Challenge::SpiralAbyss,
    Challenge::ImaginariumTheater,
    Challenge::StygianOnslaught,
];
const STARRAIL: [Challenge; 4] = [
    Challenge::MemoryOfChaos,
    Challenge::PureFiction,
    Challenge::ApocalypticShadow,
    Challenge::AnomalyArbitration,
];

/// The current period (`schedule_type=1`) with the floor lists the summaries read.
const CURRENT_ALL: &[(&str, &str)] = &[("schedule_type", "1"), ("need_all", "true")];
const CURRENT: &[(&str, &str)] = &[("schedule_type", "1")];
const NO_DETAIL: &[(&str, &str)] = &[("need_detail", "false")];

impl Challenge {
    /// The game's modes in display order.
    pub fn for_game(game: GameKind) -> &'static [Challenge] {
        match game {
            GameKind::Genshin(_) => &GENSHIN,
            GameKind::Starrail(_) => &STARRAIL,
        }
    }

    pub fn game(self) -> GameKind {
        match self {
            Self::SpiralAbyss | Self::ImaginariumTheater | Self::StygianOnslaught => Genshin::KIND,
            Self::MemoryOfChaos
            | Self::PureFiction
            | Self::ApocalypticShadow
            | Self::AnomalyArbitration => Starrail::KIND,
        }
    }

    /// Stable identifier used by the dashboard API.
    pub fn id(self) -> &'static str {
        match self {
            Self::SpiralAbyss => "spiral_abyss",
            Self::ImaginariumTheater => "imaginarium_theater",
            Self::StygianOnslaught => "stygian_onslaught",
            Self::MemoryOfChaos => "memory_of_chaos",
            Self::PureFiction => "pure_fiction",
            Self::ApocalypticShadow => "apocalyptic_shadow",
            Self::AnomalyArbitration => "anomaly_arbitration",
        }
    }

    pub(super) fn path(self) -> &'static str {
        match self {
            Self::SpiralAbyss => "spiralAbyss",
            Self::ImaginariumTheater => "role_combat",
            Self::StygianOnslaught => "hard_challenge",
            Self::MemoryOfChaos => "challenge",
            Self::PureFiction => "challenge_story",
            Self::ApocalypticShadow => "challenge_boss",
            Self::AnomalyArbitration => "challenge_peak",
        }
    }

    pub(super) fn query(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::SpiralAbyss | Self::AnomalyArbitration => CURRENT,
            Self::ImaginariumTheater | Self::StygianOnslaught => NO_DETAIL,
            Self::MemoryOfChaos | Self::PureFiction | Self::ApocalypticShadow => CURRENT_ALL,
        }
    }

    pub(super) fn record_base(self) -> &'static str {
        match self.game() {
            GameKind::Genshin(_) => Genshin::RECORD_BASE,
            GameKind::Starrail(_) => Starrail::RECORD_BASE,
        }
    }
}
