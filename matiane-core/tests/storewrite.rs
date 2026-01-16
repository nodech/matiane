use anyhow::Result;
use chrono::{TimeZone, Utc};
use matiane_core::events::{Event, Focused, TimedEvent};
use matiane_core::store::EventWriter;
use std::fs;
use tempfile::{Builder, TempDir};

fn tmpdir(name: &str) -> TempDir {
    Builder::new()
        .prefix(&format!("matiane-core-{}", name))
        .rand_bytes(10)
        .tempdir()
        .unwrap()
}

#[tokio::test]
async fn store_write_touch() -> Result<()> {
    let dir = tmpdir("store-write-touch");

    assert_eq!(fs::read_dir(dir.path())?.count(), 0);

    let now = Utc::now();
    let pathbuf = dir.path().to_path_buf();

    EventWriter::open(pathbuf, now).await?;

    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[tokio::test]
async fn store_write_event() -> Result<()> {
    let dir = tmpdir("store-write-event");

    assert_eq!(fs::read_dir(dir.path())?.count(), 0);

    let now = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap();
    let pathbuf = dir.path().to_path_buf();
    let mut store = EventWriter::open(pathbuf, now).await?;

    let tevent = TimedEvent {
        timestamp: now,
        event: Event::Alive,
    };

    store.write(&tevent).await?;
    store.flush().await?;

    let dir = fs::read_dir(dir.path())?;
    let all: Vec<_> = dir.filter_map(Result::ok).collect();
    assert_eq!(all.len(), 1);

    let contents = fs::read_to_string(all[0].path())?;
    assert_eq!(
        contents.trim(),
        r#"{"timestamp":"2025-12-31T23:59:59Z","event":{"type":"alive"}}"#
    );

    Ok(())
}

#[tokio::test]
async fn store_write_several_events() -> Result<()> {
    let now = Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 0).unwrap();
    let dir = tmpdir("store-write-several-events");
    let pathbuf = dir.path().to_path_buf();

    let mut store = EventWriter::open(pathbuf, now).await?;

    struct TestCase {
        event: TimedEvent,
        expected: &'static str,
    }

    let tests: Vec<TestCase> = vec![
        TestCase {
            event: TimedEvent {
                timestamp: Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 1).unwrap(),
                event: Event::Alive,
            },
            expected: r#"{
                "timestamp": "2025-01-01T00:00:01Z",
                "event": { "type": "alive" }
            }"#,
        },
        TestCase {
            event: TimedEvent {
                timestamp: Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 2).unwrap(),
                event: Event::Sleep,
            },
            expected: r#"{
                "timestamp": "2025-01-01T00:00:02Z",
                "event": { "type": "sleep" }
            }"#,
        },
        TestCase {
            event: TimedEvent {
                timestamp: Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 3).unwrap(),
                event: Event::Awake,
            },
            expected: r#"{
                "timestamp": "2025-01-01T00:00:03Z",
                "event": { "type": "awake" }
            }"#,
        },
        TestCase {
            event: TimedEvent {
                timestamp: Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 4).unwrap(),
                event: Event::Idle,
            },
            expected: r#"{
                "timestamp": "2025-01-01T00:00:04Z",
                "event": { "type": "idle" }
            }"#,
        },
        TestCase {
            event: TimedEvent {
                timestamp: Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 5).unwrap(),
                event: Event::Active,
            },
            expected: r#"{
                "timestamp": "2025-01-01T00:00:05Z",
                "event": { "type": "active" }
            }"#,
        },
        TestCase {
            event: TimedEvent {
                timestamp: Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 5).unwrap(),
                event: Event::Focused(Box::new(Focused {
                    title: "This-is-title".to_string(),
                    id: "Program".to_string(),
                    pid: 111,
                })),
            },
            expected: r#"
            {
                "timestamp": "2025-01-01T00:00:05Z",
                "event": {
                    "type": "focused",
                    "data": {
                        "title": "This-is-title",
                        "id": "Program",
                        "pid": 111
                    }
                }
            }
            "#,
        },
    ];

    for test in &tests {
        store.write(&test.event).await?;
    }

    store.flush().await?;

    let contents = fs::read_to_string(dir.path().join("20250101.log"))?;
    let lines: Vec<_> = contents.lines().collect();

    assert_eq!(lines.len(), tests.len());

    for (i, test) in tests.iter().enumerate() {
        let line = lines[i];
        let expected_str =
            test.expected.to_string().replace("\n", "").replace(" ", "");
        assert_eq!(line, expected_str);
    }

    Ok(())
}

#[tokio::test]
async fn store_rotate_on_write() -> Result<()> {
    let now = Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, 0).unwrap();
    let dir = tmpdir("store-write-rotate");
    let pathbuf = dir.path().to_path_buf();

    let mut store = EventWriter::open(pathbuf, now).await?;

    // five events first day,
    // five events the next.
    for i in 1..=5 {
        let now = Utc.with_ymd_and_hms(2025, 01, 01, 0, 0, i).unwrap();
        let event = TimedEvent {
            timestamp: now,
            event: Event::Alive,
        };

        store.write(&event).await?;
    }

    for i in 1..=5 {
        let now = Utc.with_ymd_and_hms(2025, 01, 02, 0, 0, i).unwrap();
        let event = TimedEvent {
            timestamp: now,
            event: Event::Alive,
        };

        store.write(&event).await?;
    }

    store.flush().await?;

    let dirs_count = fs::read_dir(dir.path())?.count();
    assert_eq!(dirs_count, 2);

    let contents_day1 = fs::read_to_string(dir.path().join("20250101.log"))?;
    let contents_day2 = fs::read_to_string(dir.path().join("20250102.log"))?;

    assert_eq!(contents_day1.lines().count(), 5);
    assert_eq!(contents_day2.lines().count(), 5);

    Ok(())
}
