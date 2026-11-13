//! The calendars' publishers, asked whether they have changed, pinned against
//! a local stub, and what is kept read back (#809).
//!
//! **gov.uk answers `If-None-Match` with a `304`**, and the school's feed
//! `If-Modified-Since` (both checked 2026-11-13). The bodies here are
//! invented.

use application::{Calendar, CalendarPublisher as _, CalendarStore as _, HolidayCalendar as _};
use infrastructure::{BankHolidays, SqliteCalendarStore, connect};
use jiff::civil::date;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

const BANK_HOLIDAYS: &str = r#"{"england-and-wales": {"division": "england-and-wales", "events": [
    {"title": "Christmas Day", "date": "2026-12-25", "notes": "", "bunting": true}
]}}"#;

const SCHOOL: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261221\r\nDTEND;VALUE=DATE:20270105\r\nSUMMARY:Half term\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

/// **Served with its tag the first time, and unchanged when asked with it**:
/// what a run and the run after it find.
#[test]
fn a_publisher_asked_with_its_tag_answers_unchanged() {
    runtime().expect("a runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/bank-holidays.json"))
            .and(header("if-none-match", "\"v1\""))
            .respond_with(ResponseTemplate::new(304))
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/bank-holidays.json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_body_string(BANK_HOLIDAYS),
            )
            .with_priority(2)
            .mount(&server)
            .await;
        let publisher = BankHolidays::new(format!("{}/bank-holidays.json", server.uri()));

        let first = publisher
            .served_since(None)
            .await
            .expect("an answer")
            .expect("served");
        assert_eq!(first.etag.as_deref(), Some("\"v1\""));
        assert_eq!(
            publisher
                .served_since(Some(&first))
                .await
                .expect("an answer"),
            None
        );
    });
}

/// **A body it cannot read is not served**, so what was kept stands.
#[test]
fn a_body_that_cannot_be_read_is_not_served() {
    runtime().expect("a runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/bank-holidays.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>down</html>"))
            .mount(&server)
            .await;
        let publisher = BankHolidays::new(format!("{}/bank-holidays.json", server.uri()));
        assert!(publisher.served_since(None).await.is_err());
    });
}

/// **What is kept is what is read**, and a store asked before anything was
/// fetched has no holidays to give rather than none.
#[test]
fn the_calendars_kept_are_the_holidays_read() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::TempDir::new().expect("a directory");
        let pool = connect(&directory.path().join("fitness.db"))
            .await
            .expect("a store");
        let store = SqliteCalendarStore::new(pool);
        assert!(store.holidays().await.is_err());

        let at = jiff::Timestamp::UNIX_EPOCH;
        for (calendar, body) in [
            (Calendar::School, SCHOOL),
            (Calendar::Public, BANK_HOLIDAYS),
        ] {
            let served = application::Served {
                body: body.to_owned(),
                etag: None,
                last_modified: Some("Fri, 13 Nov 2026 09:00:00 GMT".to_owned()),
            };
            store.keep(calendar, &served, at).await.expect("kept");
        }
        let holidays = store.holidays().await.expect("both kept");
        assert_eq!(holidays.public().len(), 1);
        assert_eq!(holidays.school().len(), 1);
        assert_eq!(
            holidays.touches(date(2026, 12, 22), date(2026, 12, 22)),
            Some(true)
        );

        let later = at
            .checked_add(jiff::SignedDuration::from_hours(1))
            .expect("a moment");
        store
            .unchanged(Calendar::School, later)
            .await
            .expect("asked");
        let kept = store
            .kept(Calendar::School)
            .await
            .expect("read")
            .expect("kept");
        assert_eq!(kept.asked, later);
        assert_eq!(
            kept.served.last_modified.as_deref(),
            Some("Fri, 13 Nov 2026 09:00:00 GMT")
        );
    });
}
