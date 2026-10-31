//! A small iCalendar (RFC 5545) reader/writer covering what a mail client's
//! planner needs: events with recurrence and alarms, and tasks.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Property {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Component {
    pub name: String,
    pub props: Vec<Property>,
    pub children: Vec<Component>,
}

impl Component {
    pub fn prop(&self, name: &str) -> Option<&Property> {
        self.props.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn text(&self, name: &str) -> Option<String> {
        self.prop(name).map(|p| unescape(&p.value))
    }

    pub fn props_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Property> + 'a {
        self.props.iter().filter(move |p| p.name.eq_ignore_ascii_case(name))
    }
}

fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else if !raw.is_empty() {
            lines.push(raw.to_string());
        }
    }
    lines
}

fn parse_line(line: &str) -> Option<Property> {
    // Split at the first ':' that is not inside a quoted parameter value.
    let mut in_quote = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            ':' if !in_quote => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_uppercase(), v.trim_matches('"').to_string()))
        .collect();
    Some(Property { name, params, value: value.to_string() })
}

/// Parse every top-level component (normally one VCALENDAR or VCARD).
pub fn parse(text: &str) -> Vec<Component> {
    let mut stack: Vec<Component> = Vec::new();
    let mut out = Vec::new();
    for line in unfold(text) {
        let Some(p) = parse_line(&line) else { continue };
        match p.name.as_str() {
            "BEGIN" => stack.push(Component { name: p.value.to_ascii_uppercase(), ..Default::default() }),
            "END" => {
                if let Some(done) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(done),
                        None => out.push(done),
                    }
                }
            }
            _ => {
                if let Some(cur) = stack.last_mut() {
                    cur.props.push(p);
                }
            }
        }
    }
    out
}

pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

/// Fold a content line at 75 octets.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut len = 0;
    for c in line.chars() {
        let w = c.len_utf8();
        if len + w > 75 {
            out.push_str("\r\n ");
            len = 1;
        }
        out.push(c);
        len += w;
    }
    out.push_str("\r\n");
    out
}

/// A parsed date-time and whether it was a DATE (all-day) value.
pub fn parse_datetime(p: &Property) -> Option<(DateTime<Utc>, bool)> {
    let v = p.value.trim();
    if p.param("VALUE").is_some_and(|x| x.eq_ignore_ascii_case("DATE")) || v.len() == 8 {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        return Some((Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?), true));
    }
    if let Some(z) = v.strip_suffix('Z') {
        let n = NaiveDateTime::parse_from_str(z, "%Y%m%dT%H%M%S").ok()?;
        return Some((Utc.from_utc_datetime(&n), false));
    }
    let n = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    let local = match p.param("TZID").and_then(|t| t.trim_start_matches('/').parse::<chrono_tz::Tz>().ok()) {
        Some(tz) => tz.from_local_datetime(&n).earliest()?.with_timezone(&Utc),
        None => chrono::Local.from_local_datetime(&n).earliest()?.with_timezone(&Utc),
    };
    Some((local, false))
}

/// Parse an RFC 5545 duration such as `-PT15M` or `P1DT2H`.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (0i64, String::new(), false);
    for c in s.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match (c, in_time) {
                    ('W', _) => n * 7 * 86400,
                    ('D', _) => n * 86400,
                    ('H', true) => n * 3600,
                    ('M', true) => n * 60,
                    ('S', true) => n,
                    _ => return None,
                };
            }
        }
    }
    Some(Duration::seconds(if neg { -total } else { total }))
}

fn format_utc(dt: DateTime<Utc>) -> String {
    dt.format("%Y%m%dT%H%M%SZ").to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    /// Minutes before the start at which to remind, if the event has an alarm.
    pub alarm_minutes: Option<i64>,
    /// DTSTART/RRULE/EXDATE lines for recurrence expansion.
    #[serde(skip)]
    pub recurrence: Option<String>,
    pub recurring: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Todo {
    pub uid: String,
    pub summary: String,
    pub due: Option<DateTime<Utc>>,
    pub completed: bool,
}

fn event_from(c: &Component) -> Option<Event> {
    let start_prop = c.prop("DTSTART")?;
    let (start, all_day) = parse_datetime(start_prop)?;
    let end = match c.prop("DTEND").and_then(parse_datetime) {
        Some((e, _)) => e,
        None => match c.prop("DURATION").and_then(|d| parse_duration(&d.value)) {
            Some(d) => start + d,
            None if all_day => start + Duration::days(1),
            None => start,
        },
    };
    let alarm_minutes = c
        .children
        .iter()
        .filter(|a| a.name == "VALARM")
        .filter_map(|a| a.prop("TRIGGER"))
        .filter(|t| !t.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE-TIME")))
        .filter_map(|t| parse_duration(&t.value))
        .map(|d| -d.num_minutes())
        .next();
    let rrules: Vec<&Property> = c.props_named("RRULE").collect();
    let recurrence = (!rrules.is_empty()).then(|| {
        let mut s = String::new();
        // rrule wants a DTSTART it understands: UTC, or local time with a TZID.
        if all_day {
            s.push_str(&format!("DTSTART:{}\n", start.format("%Y%m%dT000000Z")));
        } else {
            s.push_str(&format!("DTSTART:{}\n", format_utc(start)));
        }
        for r in rrules {
            s.push_str(&format!("RRULE:{}\n", r.value));
        }
        for ex in c.props_named("EXDATE") {
            for v in ex.value.split(',') {
                let p = Property { name: "EXDATE".into(), params: ex.params.clone(), value: v.into() };
                if let Some((d, _)) = parse_datetime(&p) {
                    s.push_str(&format!("EXDATE:{}\n", format_utc(d)));
                }
            }
        }
        s
    });
    Some(Event {
        uid: c.text("UID").unwrap_or_default(),
        summary: c.text("SUMMARY").unwrap_or_default(),
        description: c.text("DESCRIPTION").unwrap_or_default(),
        location: c.text("LOCATION").unwrap_or_default(),
        start,
        end,
        all_day,
        alarm_minutes,
        recurring: recurrence.is_some(),
        recurrence,
    })
}

/// Events in a calendar object. Overridden instances (RECURRENCE-ID) are skipped.
pub fn events(ics: &str) -> Vec<Event> {
    parse(ics)
        .iter()
        .flat_map(|cal| cal.children.iter())
        .filter(|c| c.name == "VEVENT" && c.prop("RECURRENCE-ID").is_none())
        .filter(|c| !c.prop("STATUS").is_some_and(|s| s.value.eq_ignore_ascii_case("CANCELLED")))
        .filter_map(event_from)
        .collect()
}

pub fn todos(ics: &str) -> Vec<Todo> {
    parse(ics)
        .iter()
        .flat_map(|cal| cal.children.iter())
        .filter(|c| c.name == "VTODO")
        .map(|c| Todo {
            uid: c.text("UID").unwrap_or_default(),
            summary: c.text("SUMMARY").unwrap_or_default(),
            due: c.prop("DUE").and_then(parse_datetime).map(|(d, _)| d),
            completed: c.prop("STATUS").is_some_and(|s| s.value.eq_ignore_ascii_case("COMPLETED"))
                || c.prop("COMPLETED").is_some(),
        })
        .collect()
}

/// Occurrences of an event overlapping `[from, to)`.
pub fn occurrences(ev: &Event, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let len = ev.end - ev.start;
    let Some(rec) = &ev.recurrence else {
        return if ev.start < to && ev.end > from { vec![(ev.start, ev.end)] } else { vec![] };
    };
    let Ok(set) = rec.parse::<rrule::RRuleSet>() else {
        return if ev.start < to && ev.end > from { vec![(ev.start, ev.end)] } else { vec![] };
    };
    let utc = rrule::Tz::UTC;
    let after = (from - len - Duration::seconds(1)).with_timezone(&utc);
    let before = to.with_timezone(&utc);
    set.after(after)
        .before(before)
        .all(2000)
        .dates
        .into_iter()
        .map(|d| d.with_timezone(&Utc))
        .map(|s| (s, s + len))
        .filter(|(s, e)| *s < to && *e > from)
        .collect()
}

fn wrap_calendar(body: &str) -> String {
    let mut s = String::new();
    s.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Moho Mail//EN\r\nCALSCALE:GREGORIAN\r\n");
    s.push_str(body);
    s.push_str("END:VCALENDAR\r\n");
    s
}

pub fn new_uid() -> String {
    format!("{}@moho-mail", uuid::Uuid::new_v4())
}

/// Serialize a single event as a calendar object.
pub fn event_to_ics(ev: &Event) -> String {
    let mut b = String::from("BEGIN:VEVENT\r\n");
    b.push_str(&fold(&format!("UID:{}", ev.uid)));
    b.push_str(&fold(&format!("DTSTAMP:{}", format_utc(Utc::now()))));
    if ev.all_day {
        b.push_str(&fold(&format!("DTSTART;VALUE=DATE:{}", ev.start.format("%Y%m%d"))));
        b.push_str(&fold(&format!("DTEND;VALUE=DATE:{}", ev.end.format("%Y%m%d"))));
    } else {
        b.push_str(&fold(&format!("DTSTART:{}", format_utc(ev.start))));
        b.push_str(&fold(&format!("DTEND:{}", format_utc(ev.end))));
    }
    b.push_str(&fold(&format!("SUMMARY:{}", escape(&ev.summary))));
    if !ev.location.is_empty() {
        b.push_str(&fold(&format!("LOCATION:{}", escape(&ev.location))));
    }
    if !ev.description.is_empty() {
        b.push_str(&fold(&format!("DESCRIPTION:{}", escape(&ev.description))));
    }
    if let Some(m) = ev.alarm_minutes {
        b.push_str("BEGIN:VALARM\r\nACTION:DISPLAY\r\n");
        b.push_str(&fold(&format!("DESCRIPTION:{}", escape(&ev.summary))));
        b.push_str(&fold(&format!("TRIGGER:-PT{}M", m.max(0))));
        b.push_str("END:VALARM\r\n");
    }
    b.push_str("END:VEVENT\r\n");
    wrap_calendar(&b)
}

pub fn todo_to_ics(t: &Todo) -> String {
    let mut b = String::from("BEGIN:VTODO\r\n");
    b.push_str(&fold(&format!("UID:{}", t.uid)));
    b.push_str(&fold(&format!("DTSTAMP:{}", format_utc(Utc::now()))));
    b.push_str(&fold(&format!("SUMMARY:{}", escape(&t.summary))));
    if let Some(d) = t.due {
        b.push_str(&fold(&format!("DUE:{}", format_utc(d))));
    }
    if t.completed {
        b.push_str("STATUS:COMPLETED\r\n");
        b.push_str(&fold(&format!("COMPLETED:{}", format_utc(Utc::now()))));
    } else {
        b.push_str("STATUS:NEEDS-ACTION\r\n");
    }
    b.push_str("END:VTODO\r\n");
    wrap_calendar(&b)
}

/// Change a task's completion inside an existing object, keeping other properties.
pub fn set_todo_completed(ics: &str, completed: bool) -> String {
    let mut out = String::new();
    let mut in_todo = false;
    for line in unfold(ics) {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VTODO" {
            in_todo = true;
        }
        if in_todo && (upper.starts_with("STATUS") || upper.starts_with("COMPLETED") || upper.starts_with("PERCENT-COMPLETE")) {
            continue;
        }
        if upper == "END:VTODO" && in_todo {
            if completed {
                out.push_str("STATUS:COMPLETED\r\nPERCENT-COMPLETE:100\r\n");
                out.push_str(&fold(&format!("COMPLETED:{}", format_utc(Utc::now()))));
            } else {
                out.push_str("STATUS:NEEDS-ACTION\r\n");
            }
            in_todo = false;
        }
        out.push_str(&fold(&line));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:abc\r\n\
DTSTART;TZID=America/New_York:20261026T090000\r\nDTEND;TZID=America/New_York:20261026T100000\r\n\
SUMMARY:Stand-up\\, daily\r\nDESCRIPTION:line one\\nline two that is long enough to be \r\n folded\r\n\
RRULE:FREQ=DAILY;COUNT=5\r\nEXDATE;TZID=America/New_York:20261028T090000\r\n\
BEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:allday\r\nDTSTART;VALUE=DATE:20261031\r\nSUMMARY:Halloween\r\nEND:VEVENT\r\n\
BEGIN:VTODO\r\nUID:t1\r\nSUMMARY:Pay rent\r\nDUE:20261101T170000Z\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";

    #[test]
    fn parses_events_and_todos() {
        let ev = events(ICS);
        assert_eq!(ev.len(), 2);
        let s = &ev[0];
        assert_eq!(s.summary, "Stand-up, daily");
        assert_eq!(s.description, "line one\nline two that is long enough to be folded");
        assert_eq!(s.start.to_rfc3339(), "2026-10-26T13:00:00+00:00");
        assert_eq!(s.alarm_minutes, Some(15));
        assert!(ev[1].all_day);
        assert_eq!((ev[1].end - ev[1].start).num_days(), 1);
        let t = todos(ICS);
        assert_eq!(t[0].summary, "Pay rent");
        assert!(!t[0].completed);
    }

    #[test]
    fn expands_recurrence_with_exdate() {
        let ev = &events(ICS)[0];
        let from = Utc.with_ymd_and_hms(2026, 10, 25, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap();
        let occ = occurrences(ev, from, to);
        // 5 occurrences minus the excluded 28th.
        assert_eq!(occ.len(), 4);
        assert!(occ.iter().all(|(s, e)| (*e - *s).num_minutes() == 60));
        let narrow = occurrences(ev, Utc.with_ymd_and_hms(2026, 10, 29, 0, 0, 0).unwrap(), Utc.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap());
        assert_eq!(narrow.len(), 1);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("-PT15M").unwrap().num_minutes(), -15);
        assert_eq!(parse_duration("P1DT2H").unwrap().num_hours(), 26);
        assert_eq!(parse_duration("P2W").unwrap().num_days(), 14);
        assert!(parse_duration("garbage").is_none());
    }

    #[test]
    fn round_trips_and_toggles() {
        let ev = Event {
            uid: new_uid(),
            summary: "Lunch; with, Bob".into(),
            description: "a\nb".into(),
            location: String::new(),
            start: Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 11, 2, 13, 0, 0).unwrap(),
            all_day: false,
            alarm_minutes: Some(10),
            recurrence: None,
            recurring: false,
        };
        let back = &events(&event_to_ics(&ev))[0];
        assert_eq!(back, &ev);
        let t = Todo { uid: "x".into(), summary: "Do it".into(), due: None, completed: false };
        let done = set_todo_completed(&todo_to_ics(&t), true);
        assert!(todos(&done)[0].completed);
        assert!(!todos(&set_todo_completed(&done, false))[0].completed);
    }
}
