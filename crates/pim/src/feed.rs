//! Read-only iCalendar subscriptions: a calendar published at a URL, such as
//! Google Calendar's "secret address in iCal format".

use std::collections::HashMap;
use std::time::Duration;

use crate::Error;

/// The address to fetch: `webcal://` is plain HTTPS.
pub fn http_url(url: &str) -> Result<String, Error> {
    let url = url.trim();
    let fixed = match url.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("webcal") || scheme.eq_ignore_ascii_case("webcals") => {
            format!("https://{rest}")
        }
        Some((scheme, _)) if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") => url.to_string(),
        _ => return Err(Error::Other("enter the calendar's address, starting with https:// or webcal://".into())),
    };
    url::Url::parse(&fixed).map_err(|e| Error::Other(format!("invalid address: {e}")))?;
    Ok(fixed)
}

/// Download the feed.
pub async fn fetch(url: &str) -> Result<String, Error> {
    let http = reqwest::Client::builder()
        .user_agent("Moho Mail")
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| Error::Other(e.to_string()))?;
    let resp = http.get(http_url(url)?).send().await.map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Other(format!("the calendar address answered {status}")));
    }
    let text = resp.text().await.map_err(|e| Error::Network(e.to_string()))?;
    if !text.trim_start().starts_with("BEGIN:VCALENDAR") {
        return Err(Error::Other("that address didn't return a calendar (iCal) file".into()));
    }
    Ok(text)
}

/// The calendar's own name (`X-WR-CALNAME`), if it gives one.
pub fn name(ics: &str) -> Option<String> {
    unfold(ics)
        .into_iter()
        .find_map(|l| l.strip_prefix("X-WR-CALNAME:").map(|v| crate::ical::unescape(v.trim())))
        .filter(|n| !n.is_empty())
}

/// Lines with RFC 5545 folding undone.
fn unfold(ics: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in ics.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match (raw.strip_prefix([' ', '\t']), lines.last_mut()) {
            (Some(rest), Some(prev)) => prev.push_str(rest),
            _ if raw.is_empty() => {}
            _ => lines.push(raw.to_string()),
        }
    }
    lines
}

/// Split a feed into one calendar object per UID, each carrying the feed's
/// time zones, so a subscription is stored like a DAV collection.
pub fn split(ics: &str) -> Vec<(String, String)> {
    let mut zones = String::new();
    let mut order: Vec<String> = Vec::new();
    let mut by_uid: HashMap<String, String> = HashMap::new();
    let mut block: Vec<String> = Vec::new();
    let mut depth = 0usize;
    for line in unfold(ics) {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("BEGIN:") {
            depth += 1;
        }
        if depth >= 2 {
            block.push(line.clone());
        }
        if upper.starts_with("END:") {
            if depth == 2 {
                let text: String = block.iter().map(|l| format!("{l}\r\n")).collect();
                let kind = block[0].to_ascii_uppercase();
                if kind == "BEGIN:VTIMEZONE" {
                    zones.push_str(&text);
                } else if kind == "BEGIN:VEVENT" || kind == "BEGIN:VTODO" {
                    let uid = block
                        .iter()
                        .find_map(|l| {
                            let (name, value) = l.split_once(':')?;
                            name.split(';').next()?.eq_ignore_ascii_case("UID").then(|| value.trim().to_string())
                        })
                        .unwrap_or_else(|| format!("no-uid-{}", order.len()));
                    if !by_uid.contains_key(&uid) {
                        order.push(uid.clone());
                    }
                    by_uid.entry(uid).or_default().push_str(&text);
                }
                block.clear();
            }
            depth = depth.saturating_sub(1);
        }
    }
    order
        .into_iter()
        .map(|uid| {
            let body = &by_uid[&uid];
            let data = format!(
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Moho Mail//Subscription//EN\r\n{zones}{body}END:VCALENDAR\r\n"
            );
            (uid, data)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ical;

    const FEED: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nX-WR-CALNAME:Work\\, shared\r\n\
BEGIN:VTIMEZONE\r\nTZID:America/New_York\r\nBEGIN:STANDARD\r\nDTSTART:19701101T020000\r\nTZOFFSETFROM:-0400\r\nTZOFFSETTO:-0500\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:weekly@x\r\nDTSTART;TZID=America/New_York:20261102T090000\r\nDTEND;TZID=America/New_York:20261102T093000\r\nRRULE:FREQ=WEEKLY\r\nSUMMARY:Stand\r\n -up\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:once@x\r\nDTSTART:20261105T150000Z\r\nDTEND:20261105T160000Z\r\nSUMMARY:Dentist\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:weekly@x\r\nRECURRENCE-ID;TZID=America/New_York:20261109T090000\r\nDTSTART;TZID=America/New_York:20261109T100000\r\nDTEND;TZID=America/New_York:20261109T103000\r\nSUMMARY:Stand-up (moved)\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn splits_by_uid_and_keeps_time_zones() {
        let objects = split(FEED);
        assert_eq!(objects.iter().map(|(u, _)| u.as_str()).collect::<Vec<_>>(), ["weekly@x", "once@x"]);
        let (_, weekly) = &objects[0];
        assert!(weekly.contains("BEGIN:VTIMEZONE") && weekly.contains("RECURRENCE-ID"));
        let events = ical::events(weekly);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Stand-up");
        assert!(events[0].recurring);
        assert_eq!(ical::events(&objects[1].1)[0].summary, "Dentist");
    }

    #[test]
    fn calendar_name_and_addresses() {
        assert_eq!(name(FEED).as_deref(), Some("Work, shared"));
        assert_eq!(http_url("webcal://calendar.google.com/x/basic.ics").unwrap(), "https://calendar.google.com/x/basic.ics");
        assert!(http_url("https://example.com/a.ics").is_ok());
        assert!(http_url("calendar.google.com").is_err());
    }
}
