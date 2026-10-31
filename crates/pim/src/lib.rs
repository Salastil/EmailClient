//! Calendars, tasks and contacts: iCalendar/vCard, CalDAV/CardDAV,
//! Microsoft Graph calendars, iCal subscriptions, Nextcloud sign-in and the
//! local store.

pub mod dav;
pub mod feed;
pub mod graph;
pub mod ical;
pub mod nextcloud;
pub mod store;
pub mod vcard;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sign-in failed: {0}")]
    Auth(String),
    #[error("cannot reach the server: {0}")]
    Network(String),
    #[error("server error: {0}")]
    Protocol(String),
    #[error("local storage error: {0}")]
    Store(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
