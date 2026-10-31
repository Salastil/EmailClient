//! Microsoft 365 / Outlook.com calendars via Microsoft Graph (Outlook has no CalDAV).

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::ical::Event;
use crate::{Error, Result};

const BASE: &str = "https://graph.microsoft.com/v1.0";

#[derive(Debug, Clone)]
pub struct GraphCalendar {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub writable: bool,
}

#[derive(Deserialize)]
struct Page<T> {
    value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    next: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCalendar {
    id: String,
    name: String,
    #[serde(default)]
    hex_color: Option<String>,
    #[serde(default)]
    can_edit: bool,
}

#[derive(Deserialize)]
struct DateTimeTz {
    #[serde(rename = "dateTime")]
    date_time: String,
}

#[derive(Deserialize)]
struct Body {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct Location {
    #[serde(rename = "displayName", default)]
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    id: String,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    body: Option<Body>,
    #[serde(default)]
    location: Option<Location>,
    start: DateTimeTz,
    end: DateTimeTz,
    #[serde(default)]
    is_all_day: bool,
    #[serde(default)]
    is_reminder_on: bool,
    #[serde(default)]
    reminder_minutes_before_start: Option<i64>,
    #[serde(default)]
    series_master_id: Option<String>,
    #[serde(default)]
    is_cancelled: bool,
}

fn parse_utc(s: &str) -> Option<DateTime<Utc>> {
    let s = s.split('.').next()?;
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok().map(|n| Utc.from_utc_datetime(&n))
}

pub struct Client {
    http: reqwest::Client,
    token: String,
}

impl Client {
    pub fn new(token: &str) -> Self {
        Self { http: reqwest::Client::new(), token: token.into() }
    }

    async fn get_all<T: for<'de> Deserialize<'de>>(&self, url: String) -> Result<Vec<T>> {
        let mut out = Vec::new();
        let mut next = Some(url);
        while let Some(u) = next {
            let resp = self
                .http
                .get(&u)
                .bearer_auth(&self.token)
                .header("Prefer", "outlook.timezone=\"UTC\"")
                .send()
                .await
                .map_err(|e| Error::Network(e.to_string()))?;
            if resp.status() == 401 || resp.status() == 403 {
                return Err(Error::Auth(resp.status().to_string()));
            }
            if !resp.status().is_success() {
                return Err(Error::Protocol(resp.status().to_string()));
            }
            let page: Page<T> = resp.json().await.map_err(|e| Error::Protocol(e.to_string()))?;
            out.extend(page.value);
            next = page.next;
        }
        Ok(out)
    }

    pub async fn calendars(&self) -> Result<Vec<GraphCalendar>> {
        let raw: Vec<RawCalendar> = self.get_all(format!("{BASE}/me/calendars")).await?;
        Ok(raw
            .into_iter()
            .map(|c| GraphCalendar {
                id: c.id,
                name: c.name,
                color: c.hex_color.filter(|h| h.starts_with('#')),
                writable: c.can_edit,
            })
            .collect())
    }

    /// Event instances (recurrences already expanded) in a time range, keyed by Graph id.
    pub async fn events(&self, calendar_id: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<(String, Event)>> {
        let url = format!(
            "{BASE}/me/calendars/{calendar_id}/calendarView?startDateTime={}&endDateTime={}&$top=200",
            from.format("%Y-%m-%dT%H:%M:%SZ"),
            to.format("%Y-%m-%dT%H:%M:%SZ")
        );
        let raw: Vec<RawEvent> = self.get_all(url).await?;
        Ok(raw
            .into_iter()
            .filter(|e| !e.is_cancelled)
            .filter_map(|e| {
                let start = parse_utc(&e.start.date_time)?;
                let end = parse_utc(&e.end.date_time)?;
                Some((
                    e.id.clone(),
                    Event {
                        uid: e.series_master_id.clone().map(|m| format!("{m}:{}", e.id)).unwrap_or(e.id),
                        summary: e.subject.unwrap_or_default(),
                        description: e.body.map(|b| strip_html(&b.content)).unwrap_or_default(),
                        location: e.location.map(|l| l.display_name).unwrap_or_default(),
                        start,
                        end,
                        all_day: e.is_all_day,
                        alarm_minutes: e.is_reminder_on.then_some(e.reminder_minutes_before_start.unwrap_or(15)),
                        recurrence: None,
                        recurring: e.series_master_id.is_some(),
                    },
                ))
            })
            .collect())
    }

    pub async fn create_event(&self, calendar_id: &str, ev: &Event) -> Result<String> {
        let fmt = |d: DateTime<Utc>| if ev.all_day { d.format("%Y-%m-%dT00:00:00").to_string() } else { d.format("%Y-%m-%dT%H:%M:%S").to_string() };
        let body = json!({
            "subject": ev.summary,
            "body": { "contentType": "text", "content": ev.description },
            "location": { "displayName": ev.location },
            "start": { "dateTime": fmt(ev.start), "timeZone": "UTC" },
            "end": { "dateTime": fmt(ev.end), "timeZone": "UTC" },
            "isAllDay": ev.all_day,
            "isReminderOn": ev.alarm_minutes.is_some(),
            "reminderMinutesBeforeStart": ev.alarm_minutes.unwrap_or(0),
        });
        let resp = self
            .http
            .post(format!("{BASE}/me/calendars/{calendar_id}/events"))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Protocol(format!("creating the event failed: {}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| Error::Protocol(e.to_string()))?;
        Ok(v["id"].as_str().unwrap_or_default().to_string())
    }

    pub async fn delete_event(&self, id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(format!("{BASE}/me/events/{id}"))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        if !resp.status().is_success() && resp.status() != 404 {
            return Err(Error::Protocol(format!("deleting the event failed: {}", resp.status())));
        }
        Ok(())
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_graph_times_and_html() {
        assert_eq!(parse_utc("2026-10-28T12:00:00.0000000").unwrap().to_rfc3339(), "2026-10-28T12:00:00+00:00");
        assert_eq!(strip_html("<p>Hi&nbsp;<b>there</b></p>"), "Hi there");
    }
}
