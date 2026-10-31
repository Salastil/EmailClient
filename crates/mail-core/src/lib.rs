//! Core of Moho Mail: account/message model, threading, notification
//! policy, sanitizing and XDG-compliant storage locations.

pub mod accounts;
pub mod model;
pub mod notify;
pub mod parse;
pub mod paths;
pub mod sanitize;
pub mod secrets;
pub mod settings;
pub mod store;
pub mod threading;
