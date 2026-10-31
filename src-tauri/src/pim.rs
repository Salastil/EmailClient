//! Calendars, tasks and contacts. Filled in by the CalDAV/CardDAV integration.

use tauri::AppHandle;

/// Lower-cased addresses from the user's address books.
pub fn contact_emails(_app: &AppHandle) -> Vec<String> {
    Vec::new()
}

pub fn complete_contacts(_app: &AppHandle, _prefix: &str, _limit: usize) -> Vec<mail_core::model::Address> {
    Vec::new()
}

pub fn init(_app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
