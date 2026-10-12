use serde::{Deserialize, Serialize};

/// Links attached to a statistic in `field_ext_map`.
#[derive(Debug, Serialize, Deserialize)]
pub struct FieldLink {
    pub link: String,
    pub backup_link: String,
}
