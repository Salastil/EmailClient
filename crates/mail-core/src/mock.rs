//! Demo data used until the sync engine lands, so the UI can be developed
//! and exercised end to end.

use chrono::{DateTime, Duration, TimeZone, Utc};

use crate::model::*;
use crate::notify::NotifyMode;

pub fn accounts() -> Vec<Account> {
    let a = |id: &str, name: &str, email: &str, provider, color: &str, unread| Account {
        id: id.into(),
        name: name.into(),
        email: email.into(),
        provider,
        color: color.into(),
        unread,
        notify: NotifyMode::All,
    };
    vec![
        a("gmail", "Gmail", "you@gmail.com", Provider::Gmail, "#ea4335", 3),
        a("outlook", "Hotmail", "you@hotmail.com", Provider::Outlook, "#0078d4", 0),
        a("cockli", "cock.li", "you@cock.li", Provider::Cockli, "#8e24aa", 1),
        a("proton", "Proton", "you@proton.me", Provider::Proton, "#6d4aff", 0),
        a("fastmail", "Fastmail", "you@fastmail.com", Provider::Fastmail, "#2e7d32", 0),
    ]
}

pub fn folders(account_id: &str) -> Vec<Folder> {
    let f = |id: &str, name: &str, role, unread, total| Folder {
        id: format!("{account_id}:{id}"),
        account_id: account_id.into(),
        name: name.into(),
        role,
        unread,
        total,
        children: vec![],
    };
    let inbox_unread = accounts().iter().find(|a| a.id == account_id).map_or(0, |a| a.unread);
    let mut out = vec![
        f("inbox", "Inbox", FolderRole::Inbox, inbox_unread, inbox(account_id).len() as u32),
        f("starred", "Starred", FolderRole::Starred, 0, 2),
        f("sent", "Sent", FolderRole::Sent, 0, 40),
        f("drafts", "Drafts", FolderRole::Drafts, 0, 2),
        f("archive", "Archive", FolderRole::Archive, 0, 812),
        f("spam", "Spam", FolderRole::Spam, 4, 4),
        f("trash", "Trash", FolderRole::Trash, 0, 17),
    ];
    let mut projects = f("projects", "Projects", FolderRole::Other, 0, 12);
    projects.children = vec![
        f("projects/moho", "Moho", FolderRole::Other, 2, 9),
        f("projects/twikit", "twikit", FolderRole::Other, 0, 3),
    ];
    out.push(f("receipts", "Receipts", FolderRole::Other, 4, 31));
    out.push(projects);
    if account_id == "gmail" {
        out.push(f("label/bills", "Bills", FolderRole::Label, 0, 6));
        out.push(f("label/family", "Family", FolderRole::Label, 1, 20));
    }
    out
}

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
}

struct Seed {
    id: &'static str,
    irt: Option<&'static str>,
    from: (&'static str, &'static str),
    subject: &'static str,
    date: DateTime<Utc>,
    unread: bool,
    flagged: bool,
    attach: bool,
}

fn seeds() -> Vec<Seed> {
    let s = |id, irt, from, subject, date, unread, flagged, attach| Seed {
        id, irt, from, subject, date, unread, flagged, attach,
    };
    vec![
        s("privacy-1", None, ("Contact", "contact@service.example"), "Important updates to our Privacy Policy", at(2026, 10, 17, 2, 45), true, false, false),
        s("twikit-1", None, ("alex.p", "alex.p@getfoo.example"), "twikit + x's per-read pricing", at(2026, 9, 4, 10, 58), false, true, false),
        s("twikit-2", Some("twikit-1"), ("alex.p", "alex.p@getfoo.example"), "Re: twikit + x's per-read pricing", at(2026, 9, 23, 7, 18), false, false, false),
        s("twikit-3", Some("twikit-2"), ("alex.p", "alex.p@getfoo.example"), "Re: twikit + x's per-read pricing", at(2026, 10, 8, 10, 51), true, false, true),
        s("poast-1", None, ("Poast Matrix", "matrix@poa.example"), "[poa.st] Validate your email", at(2026, 10, 1, 8, 23), false, false, false),
        s("poast-2", None, ("Poast Matrix", "matrix@poa.example"), "[poa.st] Welcome aboard", at(2026, 9, 28, 16, 14), false, false, false),
        s("lawn-1", None, ("Dandalf", "dandalf@example.org"), "Lawn cutting Monday?", at(2026, 10, 24, 18, 2), true, false, false),
        s("lawn-2", Some("lawn-1"), ("You", "you@example.com"), "Re: Lawn cutting Monday?", at(2026, 10, 24, 19, 40), false, false, false),
        s("lawn-3", Some("lawn-2"), ("Dandalf", "dandalf@example.org"), "Re: Lawn cutting Monday?", at(2026, 10, 24, 20, 5), false, false, false),
        s("lawn-4", Some("lawn-2"), ("TheQuimbly", "quimbly@example.net"), "Re: Lawn cutting Monday?", at(2026, 10, 25, 9, 12), false, false, false),
        s("insure-1", None, ("State Farm John", "john@agent.example"), "Policy renewal — Friday call", at(2026, 10, 27, 14, 30), false, false, true),
    ]
}

pub fn inbox(account_id: &str) -> Vec<MessageSummary> {
    let offset = match account_id {
        "gmail" => 0,
        "outlook" => 2,
        "cockli" => 4,
        "proton" => 6,
        _ => 8,
    };
    let all = seeds();
    let count = if account_id == "gmail" { all.len() } else { 5 };
    (0..count)
        .map(|i| summary(account_id, &all[(offset + i) % all.len()]))
        .collect()
}

fn summary(account_id: &str, s: &Seed) -> MessageSummary {
    MessageSummary {
        id: format!("{account_id}:{}", s.id),
        account_id: account_id.into(),
        folder_id: format!("{account_id}:inbox"),
        message_id: format!("{}@mock", s.id),
        in_reply_to: s.irt.map(|p| format!("{p}@mock")),
        references: s.irt.map(|p| format!("{p}@mock")).into_iter().collect(),
        thread_hint: None,
        subject: s.subject.into(),
        from: Address { name: Some(s.from.0.into()), email: s.from.1.into() },
        date: s.date,
        unread: s.unread,
        flagged: s.flagged,
        important: s.flagged,
        has_attachments: s.attach,
    }
}

pub fn messages(folder_id: &str) -> Vec<MessageSummary> {
    let (account, folder) = folder_id.split_once(':').unwrap_or((folder_id, "inbox"));
    match folder {
        "inbox" => inbox(account),
        "starred" => inbox(account).into_iter().filter(|m| m.flagged).collect(),
        _ => {
            let mut msgs = inbox(account);
            msgs.truncate(2);
            for (i, m) in msgs.iter_mut().enumerate() {
                m.folder_id = folder_id.into();
                m.date -= Duration::days(30 * (i as i64 + 1));
            }
            msgs
        }
    }
}

pub fn message(id: &str) -> Option<Message> {
    let (account, _) = id.split_once(':')?;
    let summary = ["inbox", "spam"]
        .iter()
        .flat_map(|f| messages(&format!("{account}:{f}")))
        .find(|m| m.id == id)?;
    let attachments = if summary.has_attachments {
        vec![Attachment { name: "pricing.pdf".into(), mime: "application/pdf".into(), size: 212 * 1024 }]
    } else {
        vec![]
    };
    let body_html = Some(format!(
        r#"<div style="font-family:sans-serif;line-height:1.5">
<p>hey t,</p>
<p>Following up on <b>{subject}</b>. If you watch the free tier they cap reads hard;
the paid API is about $1.99 per thousand reads, so a full backfill would be ~60x
what we budgeted.</p>
<p>No action needed yet, just wanted it on your radar. JSON sample below.</p>
<pre style="background:#2223;padding:8px;border-radius:6px">{{"reads": 60000, "cost_usd": 119.4}}</pre>
<img src="https://tracker.example/pixel.gif" width="1" height="1">
<p>— {from}</p>
</div>"#,
        subject = summary.subject,
        from = summary.from.name.as_deref().unwrap_or(&summary.from.email),
    ));
    Some(Message {
        to: vec![Address { name: Some("You".into()), email: format!("you@{account}.example") }],
        cc: vec![],
        body_text: None,
        body_html,
        attachments,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_threads_like_the_design() {
        let threads = crate::threading::build_threads(inbox("gmail"));
        let twikit = threads.iter().find(|t| t.message.id == "gmail:twikit-1").unwrap();
        assert_eq!(twikit.reply_count, 2);
        let lawn = threads.iter().find(|t| t.message.id == "gmail:lawn-1").unwrap();
        assert_eq!(lawn.reply_count, 3);
        assert!(message("gmail:twikit-3").is_some());
    }
}
