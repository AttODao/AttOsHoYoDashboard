//! Non-GUI dashboard application: command, domain, persistence, and HTTP boundaries.
mod auth;
mod cli;
mod config;
mod db;
mod error;
mod hoyolab;
mod http;

#[cfg(test)]
mod test_support;

pub(super) use cli::run;
