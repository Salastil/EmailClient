//! Data model shared between the sync engine and the UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::notify::NotifyMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gmail,
    Outlook,
    Yahoo,
    Icloud,
    Fastmail,
    Cockli,
    Proton,
    Imap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    pub provider: Provider,
    /// Accent colour for the rail avatar, as `#rrggbb`.
    pub color: String,
    pub unread: u32,
    pub notify: NotifyMode,
    #[serde(default)]
    pub status: AccountStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "message")]
pub enum AccountStatus {
    #[default]
    Idle,
    Syncing,
    Offline(String),
    Error(String),
    /// Credentials were rejected or the OAuth grant was revoked.
    NeedsAuth(String),
}

/// IMAP special-use role (RFC 6154) of a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderRole {
    Inbox,
    Starred,
    Sent,
    Drafts,
    Archive,
    Spam,
    Trash,
    All,
    /// Gmail's Important folder (`\Important` special use).
    Important,
    Label,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub account_id: String,
    /// Server-side mailbox name.
    pub path: String,
    pub name: String,
    pub role: FolderRole,
    pub unread: u32,
    pub total: u32,
    /// Monitored as important: notifications in "Important only" mode and the
    /// tray's unread state follow these folders.
    #[serde(default)]
    pub important: bool,
    #[serde(default)]
    pub children: Vec<Folder>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: String,
    pub account_id: String,
    pub folder_id: String,
    /// RFC 5322 `Message-ID`, without angle brackets.
    pub message_id: String,
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
    /// Provider thread id such as Gmail's `X-GM-THRID`.
    pub thread_hint: Option<String>,
    pub subject: String,
    pub from: Address,
    pub date: DateTime<Utc>,
    pub unread: bool,
    pub flagged: bool,
    pub important: bool,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Position among the message's attachments, used to download it.
    pub index: usize,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub summary: MessageSummary,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub body_html: Option<String>,
    pub body_text: Option<String>,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadNode {
    pub message: MessageSummary,
    /// Total number of messages below this node.
    pub reply_count: usize,
    pub children: Vec<ThreadNode>,
}
