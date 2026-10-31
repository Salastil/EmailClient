//! Fetches Google's public US holidays calendar, which is served exactly like
//! a secret iCal address. Run with `MOHO_TEST_FEED=1 cargo test -p pim --test feed_live`.

use chrono::{Duration, Utc};
use pim::{feed, ical};

#[tokio::test]
async fn google_public_feed() {
    if std::env::var("MOHO_TEST_FEED").is_err() {
        return;
    }
    let url = "webcal://calendar.google.com/calendar/ical/en.usa%23holiday%40group.v.calendar.google.com/public/basic.ics";
    let text = feed::fetch(url).await.unwrap();
    assert!(feed::name(&text).is_some());
    let objects = feed::split(&text);
    assert!(objects.len() > 10, "only {} events", objects.len());
    let (from, to) = (Utc::now(), Utc::now() + Duration::days(366));
    let upcoming = objects
        .iter()
        .flat_map(|(_, data)| ical::events(data))
        .filter(|e| !ical::occurrences(e, from, to).is_empty())
        .count();
    assert!(upcoming > 5, "only {upcoming} events in the next year");
    println!("{:?}: {} events, {upcoming} in the next year", feed::name(&text), objects.len());
}
