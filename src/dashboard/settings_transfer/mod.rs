//! Export and import of a user's own settings as an `ahd-settings` JSON file.
pub(in crate::dashboard) mod document;
pub(in crate::dashboard) mod service;

#[cfg(test)]
#[path = "document_tests.rs"]
mod document_tests;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
