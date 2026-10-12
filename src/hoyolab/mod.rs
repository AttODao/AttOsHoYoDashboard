//! HoYoLAB game configuration, response models, and API client.

mod client;
pub mod game;
pub mod models;

pub use client::{HoyolabClient, RecordAccount, Result};
