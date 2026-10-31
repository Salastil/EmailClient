//! Runs against Radicale started by `scripts/test-servers.sh`.
//! Skipped unless MOHO_TEST_DAV is set.

use chrono::{Duration, TimeZone, Utc};
use pim::dav::{Auth, Client, Kind};
use pim::ical::{self, Event};

#[tokio::test]
async fn caldav_round_trip() {
    let Ok(base) = std::env::var("MOHO_TEST_DAV") else {
        eprintln!("MOHO_TEST_DAV not set; skipping");
        return;
    };
    let client = Client::new(Auth::Basic { user: "test".into(), password: "secret".into() });
    let cal_url = format!("{base}test/work-{}/", std::process::id());
    client.make_calendar(&cal_url, "Work").await.unwrap();

    let cals = client.discover(&base, Kind::Calendar).await.unwrap();
    let work = cals.iter().find(|c| c.url == cal_url).expect("discovered the new calendar");
    assert_eq!(work.name, "Work");
    assert!(work.writable);

    let ev = Event {
        uid: ical::new_uid(),
        summary: "Lawn cutting".into(),
        description: String::new(),
        location: "Back yard".into(),
        start: Utc.with_ymd_and_hms(2026, 10, 26, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 10, 27, 0, 0, 0).unwrap(),
        all_day: true,
        alarm_minutes: Some(60),
        recurrence: None,
        recurring: false,
    };
    let (href, _etag) = client.put(&work.url, None, &ev.uid, &ical::event_to_ics(&ev), None, Kind::Calendar).await.unwrap();

    let from = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
    let objs = client.objects(work, Some((from, from + Duration::days(60)))).await.unwrap();
    assert_eq!(objs.len(), 1);
    let back = &ical::events(&objs[0].data)[0];
    assert_eq!(back.summary, "Lawn cutting");
    assert!(back.all_day);

    // Optimistic concurrency: a stale ETag is rejected.
    let stale = client.put(&work.url, Some(&href), &ev.uid, &ical::event_to_ics(&ev), Some("\"stale\""), Kind::Calendar).await;
    assert!(stale.is_err());

    client.delete(&objs[0].href, Some(&objs[0].etag)).await.unwrap();
    assert!(client.objects(work, None).await.unwrap().is_empty());

    let bad = Client::new(Auth::Basic { user: "test".into(), password: "nope".into() });
    assert!(matches!(bad.discover(&base, Kind::Calendar).await, Err(pim::Error::Auth(_))));
}
