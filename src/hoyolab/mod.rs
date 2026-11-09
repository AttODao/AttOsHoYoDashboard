//! HoYoLAB game configuration, response models, and API client.

pub mod challenge;
mod client;
mod error;
pub mod game;
pub mod models;

pub use client::{HoyolabClient, RecordAccount};
pub use error::{Error, Result};
