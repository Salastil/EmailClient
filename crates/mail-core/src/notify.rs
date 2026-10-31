//! Decides whether a newly arrived message should raise a desktop notification.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::{FolderRole, MessageSummary};

/// Per-account notification mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyMode {
    #[default]
    All,
    ImportantOnly,
    Muted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleField {
    Sender,
    Domain,
    Subject,
}

/// A user rule that marks matching messages as important.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportanceRule {
    pub field: RuleField,
    pub contains: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifySettings {
    pub do_not_disturb: bool,
    /// Quiet hours as `[start_hour, end_hour)` in local time; may wrap midnight.
    pub quiet_hours: Option<(u8, u8)>,
    pub vip: Vec<String>,
    pub rules: Vec<ImportanceRule>,
    pub use_provider_important: bool,
    pub use_contacts: bool,
    pub use_flagged: bool,
    pub use_replied_threads: bool,
    pub muted_folders: Vec<FolderRole>,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            do_not_disturb: false,
            quiet_hours: None,
            vip: Vec::new(),
            rules: Vec::new(),
            use_provider_important: true,
            use_contacts: true,
            use_flagged: true,
            use_replied_threads: true,
            muted_folders: vec![FolderRole::Spam, FolderRole::Trash, FolderRole::Sent, FolderRole::Drafts],
        }
    }
}

/// Facts about the message that come from outside the message itself.
#[derive(Debug, Default)]
pub struct ImportanceContext<'a> {
    /// Lower-cased addresses from the user's address books.
    pub contacts: Option<&'a HashSet<String>>,
    /// The user has already replied in this message's thread.
    pub replied_in_thread: bool,
}

pub fn is_important(msg: &MessageSummary, ctx: &ImportanceContext, s: &NotifySettings) -> bool {
    let sender = msg.from.email.to_lowercase();
    let domain = sender.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
    let subject = msg.subject.to_lowercase();

    (s.use_provider_important && msg.important)
        || (s.use_flagged && msg.flagged)
        || (s.use_replied_threads && ctx.replied_in_thread)
        || (s.use_contacts && ctx.contacts.is_some_and(|c| c.contains(&sender)))
        || s.vip.iter().any(|v| v.eq_ignore_ascii_case(&sender))
        || s.rules.iter().any(|r| {
            let needle = r.contains.to_lowercase();
            !needle.is_empty()
                && match r.field {
                    RuleField::Sender => sender.contains(&needle),
                    RuleField::Domain => domain == needle || domain.ends_with(&format!(".{needle}")),
                    RuleField::Subject => subject.contains(&needle),
                }
        })
}

fn in_quiet_hours(quiet: Option<(u8, u8)>, hour: u8) -> bool {
    match quiet {
        None => false,
        Some((start, end)) if start == end => false,
        Some((start, end)) if start < end => (start..end).contains(&hour),
        Some((start, end)) => hour >= start || hour < end,
    }
}

/// Whether a new message should produce a notification.
pub fn should_notify(
    mode: NotifyMode,
    folder: FolderRole,
    msg: &MessageSummary,
    ctx: &ImportanceContext,
    settings: &NotifySettings,
    local_hour: u8,
) -> bool {
    if mode == NotifyMode::Muted
        || settings.do_not_disturb
        || in_quiet_hours(settings.quiet_hours, local_hour)
        || settings.muted_folders.contains(&folder)
    {
        return false;
    }
    mode == NotifyMode::All || is_important(msg, ctx, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Address;
    use chrono::Utc;

    fn msg(from: &str, subject: &str) -> MessageSummary {
        MessageSummary {
            id: "1".into(),
            account_id: "a".into(),
            folder_id: "inbox".into(),
            message_id: "1".into(),
            in_reply_to: None,
            references: vec![],
            thread_hint: None,
            subject: subject.into(),
            from: Address { name: None, email: from.into() },
            date: Utc::now(),
            unread: true,
            flagged: false,
            important: false,
            has_attachments: false,
        }
    }

    fn notify(mode: NotifyMode, m: &MessageSummary, s: &NotifySettings) -> bool {
        should_notify(mode, FolderRole::Inbox, m, &ImportanceContext::default(), s, 12)
    }

    #[test]
    fn modes() {
        let s = NotifySettings::default();
        let m = msg("news@shop.example", "Sale");
        assert!(notify(NotifyMode::All, &m, &s));
        assert!(!notify(NotifyMode::ImportantOnly, &m, &s));
        assert!(!notify(NotifyMode::Muted, &m, &s));
    }

    #[test]
    fn important_signals() {
        let mut s = NotifySettings::default();
        let mut m = msg("Boss@Work.example", "Status");
        assert!(!notify(NotifyMode::ImportantOnly, &m, &s));

        m.important = true;
        assert!(notify(NotifyMode::ImportantOnly, &m, &s));
        s.use_provider_important = false;
        assert!(!notify(NotifyMode::ImportantOnly, &m, &s));
        m.important = false;

        m.flagged = true;
        assert!(notify(NotifyMode::ImportantOnly, &m, &s));
        m.flagged = false;

        s.vip.push("boss@work.example".into());
        assert!(notify(NotifyMode::ImportantOnly, &m, &s));
        s.vip.clear();

        let contacts: HashSet<String> = ["boss@work.example".to_string()].into();
        let ctx = ImportanceContext { contacts: Some(&contacts), ..Default::default() };
        assert!(should_notify(NotifyMode::ImportantOnly, FolderRole::Inbox, &m, &ctx, &s, 12));

        let ctx = ImportanceContext { replied_in_thread: true, ..Default::default() };
        assert!(should_notify(NotifyMode::ImportantOnly, FolderRole::Inbox, &m, &ctx, &s, 12));
    }

    #[test]
    fn rules() {
        let mut s = NotifySettings::default();
        s.rules.push(ImportanceRule { field: RuleField::Domain, contains: "work.example".into() });
        assert!(notify(NotifyMode::ImportantOnly, &msg("a@mail.work.example", "x"), &s));
        assert!(!notify(NotifyMode::ImportantOnly, &msg("a@notwork.example", "x"), &s));
        s.rules = vec![ImportanceRule { field: RuleField::Subject, contains: "INVOICE".into() }];
        assert!(notify(NotifyMode::ImportantOnly, &msg("a@b.c", "Your invoice #3"), &s));
    }

    #[test]
    fn suppression() {
        let mut s = NotifySettings::default();
        let m = msg("a@b.c", "x");
        let ctx = ImportanceContext::default();
        assert!(!should_notify(NotifyMode::All, FolderRole::Spam, &m, &ctx, &s, 12));
        s.quiet_hours = Some((22, 7));
        assert!(!should_notify(NotifyMode::All, FolderRole::Inbox, &m, &ctx, &s, 23));
        assert!(!should_notify(NotifyMode::All, FolderRole::Inbox, &m, &ctx, &s, 3));
        assert!(should_notify(NotifyMode::All, FolderRole::Inbox, &m, &ctx, &s, 12));
        s.do_not_disturb = true;
        assert!(!should_notify(NotifyMode::All, FolderRole::Inbox, &m, &ctx, &s, 12));
    }
}
