//! Conversation threading in the spirit of JWZ's algorithm.
//!
//! A message's parent is, in order of preference: its `In-Reply-To`, the
//! closest `References` entry that is present, an earlier message with the
//! same provider thread id (Gmail `X-GM-THRID`), or an earlier message whose
//! normalised subject matches when this one is a reply/forward.

use std::collections::HashMap;

use crate::model::{MessageSummary, ThreadNode};

pub fn build_threads(messages: Vec<MessageSummary>) -> Vec<ThreadNode> {
    let n = messages.len();
    let by_id: HashMap<&str, usize> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| (m.message_id.as_str(), i))
        .collect();

    // Earliest message per provider thread id and per normalised subject.
    let mut first_by_hint: HashMap<&str, usize> = HashMap::new();
    let mut first_by_subject: HashMap<String, usize> = HashMap::new();
    for (i, m) in messages.iter().enumerate() {
        if let Some(hint) = m.thread_hint.as_deref() {
            first_by_hint
                .entry(hint)
                .and_modify(|j| if messages[*j].date > m.date { *j = i })
                .or_insert(i);
        }
        if !is_reply_subject(&m.subject) {
            first_by_subject
                .entry(normalize_subject(&m.subject))
                .and_modify(|j| if messages[*j].date > m.date { *j = i })
                .or_insert(i);
        }
    }

    let mut parent: Vec<Option<usize>> = vec![None; n];
    for (i, m) in messages.iter().enumerate() {
        let header_parent = m
            .in_reply_to
            .iter()
            .chain(m.references.iter().rev())
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .find(|&p| p != i);
        let candidate = header_parent
            .or_else(|| {
                let hint = m.thread_hint.as_deref()?;
                first_by_hint.get(hint).copied().filter(|&p| p != i)
            })
            .or_else(|| {
                if !is_reply_subject(&m.subject) {
                    return None;
                }
                let p = *first_by_subject.get(&normalize_subject(&m.subject))?;
                (p != i && messages[p].date <= m.date).then_some(p)
            });
        if let Some(p) = candidate {
            if !is_ancestor(&parent, i, p) {
                parent[i] = Some(p);
            }
        }
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match *p {
            Some(p) => children[p].push(i),
            None => roots.push(i),
        }
    }

    let mut slots: Vec<Option<MessageSummary>> = messages.into_iter().map(Some).collect();
    let mut trees: Vec<ThreadNode> = roots
        .into_iter()
        .map(|r| assemble(r, &children, &mut slots))
        .collect();
    trees.sort_by_key(|t| std::cmp::Reverse(latest(t)));
    trees
}

/// Whether `ancestor` is `node` itself or above it in the current forest.
fn is_ancestor(parent: &[Option<usize>], ancestor: usize, mut node: usize) -> bool {
    loop {
        if node == ancestor {
            return true;
        }
        match parent[node] {
            Some(p) => node = p,
            None => return false,
        }
    }
}

fn assemble(
    idx: usize,
    children: &[Vec<usize>],
    slots: &mut [Option<MessageSummary>],
) -> ThreadNode {
    let message = slots[idx].take().expect("each message is placed once");
    let mut kids: Vec<ThreadNode> = children[idx]
        .iter()
        .map(|&c| assemble(c, children, slots))
        .collect();
    kids.sort_by_key(|k| k.message.date);
    let reply_count = kids.iter().map(|k| 1 + k.reply_count).sum();
    ThreadNode { message, reply_count, children: kids }
}

fn latest(node: &ThreadNode) -> chrono::DateTime<chrono::Utc> {
    node.children
        .iter()
        .map(latest)
        .fold(node.message.date, |a, b| a.max(b))
}

const REPLY_PREFIXES: &[&str] = &["re", "fw", "fwd", "aw", "sv", "vs", "antw"];

fn strip_prefix(subject: &str) -> Option<&str> {
    let s = subject.trim_start();
    let colon = s.find(':')?;
    let head = s[..colon].trim();
    // Allow counters such as "Re[2]".
    let word = head.split('[').next().unwrap_or(head);
    REPLY_PREFIXES
        .iter()
        .any(|p| word.eq_ignore_ascii_case(p))
        .then(|| &s[colon + 1..])
}

fn is_reply_subject(subject: &str) -> bool {
    strip_prefix(subject).is_some()
}

pub fn normalize_subject(subject: &str) -> String {
    let mut s = subject;
    while let Some(rest) = strip_prefix(s) {
        s = rest;
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Address;
    use chrono::{TimeZone, Utc};

    fn msg(id: &str, subject: &str, day: u32, irt: Option<&str>, refs: &[&str]) -> MessageSummary {
        MessageSummary {
            id: id.into(),
            account_id: "a".into(),
            folder_id: "inbox".into(),
            message_id: id.into(),
            in_reply_to: irt.map(Into::into),
            references: refs.iter().map(|s| s.to_string()).collect(),
            thread_hint: None,
            subject: subject.into(),
            from: Address { name: None, email: "x@example.com".into() },
            date: Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap(),
            unread: false,
            flagged: false,
            important: false,
            has_attachments: false,
        }
    }

    #[test]
    fn links_by_in_reply_to_and_references() {
        let threads = build_threads(vec![
            msg("a", "pricing", 1, None, &[]),
            msg("b", "Re: pricing", 2, Some("a"), &["a"]),
            msg("c", "Re: pricing", 3, None, &["a", "b"]),
            msg("z", "other", 4, None, &[]),
        ]);
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].message.id, "z");
        let root = &threads[1];
        assert_eq!(root.message.id, "a");
        assert_eq!(root.reply_count, 2);
        assert_eq!(root.children[0].message.id, "b");
        assert_eq!(root.children[0].children[0].message.id, "c");
    }

    #[test]
    fn skips_missing_references() {
        let threads = build_threads(vec![
            msg("a", "x", 1, None, &[]),
            msg("c", "Re: x", 3, Some("missing"), &["a", "missing"]),
        ]);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].reply_count, 1);
    }

    #[test]
    fn falls_back_to_subject_for_replies_only() {
        let threads = build_threads(vec![
            msg("a", "Lunch?", 1, None, &[]),
            msg("b", "RE: Fwd: lunch?", 2, None, &[]),
            msg("c", "Lunch?", 3, None, &[]),
        ]);
        let a = threads.iter().find(|t| t.message.id == "a").unwrap();
        assert_eq!(a.reply_count, 1);
        assert!(threads.iter().any(|t| t.message.id == "c"));
    }

    #[test]
    fn uses_provider_thread_hint() {
        let mut a = msg("a", "one", 1, None, &[]);
        let mut b = msg("b", "two", 2, None, &[]);
        a.thread_hint = Some("t1".into());
        b.thread_hint = Some("t1".into());
        let threads = build_threads(vec![b, a]);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].message.id, "a");
    }

    #[test]
    fn breaks_reference_cycles() {
        let threads = build_threads(vec![
            msg("a", "x", 1, Some("b"), &[]),
            msg("b", "y", 2, Some("a"), &[]),
        ]);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].reply_count, 1);
    }

    #[test]
    fn normalizes_subjects() {
        assert_eq!(normalize_subject("Re: RE[2]: FWD:  Hello   World"), "hello world");
        assert_eq!(normalize_subject("Rebate: info"), "rebate: info");
    }
}
