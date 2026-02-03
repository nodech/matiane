use anyhow::Result;
use futures::{StreamExt, TryStreamExt};
use matiane_core::events::TimedEvent;
use matiane_core::store::EventReader;
use std::path::Path;
use tokio::fs;

mod util;
use util::tmpdir;

async fn prepare_files(dir: &Path) -> Result<()> {
    fs::write(
        dir.join("20260101.log"),
        json_lines![
            {
                "timestamp": "2026-01-01T20:00:00Z",
                "event": {
                    "type": "alive"
                }
            },
            {
                "timestamp": "2026-01-01T22:00:00Z",
                "event": {
                    "type": "sleep"
                }
            },
        ],
    )
    .await?;

    fs::write(
        dir.join("20260103.log"),
        json_lines![
            {
                "timestamp": "2026-01-03T05:00:00Z",
                "event": {
                    "type": "awake"
                }
            },
            {
                "timestamp": "2026-01-03T05:01:00Z",
                "event": {
                    "type": "alive"
                }
            },
        ],
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn store_read_dir() -> Result<()> {
    let dir = tmpdir("store-read-dir");

    let files = EventReader::list_files(dir.path()).await?;
    assert_eq!(files.len(), 0);

    prepare_files(dir.path()).await?;

    let files = EventReader::list_files(dir.path()).await?;
    assert_eq!(files.len(), 2);

    fs::write(dir.path().join("LOCK"), "").await?;

    let files = EventReader::list_files(dir.path()).await?;
    assert_eq!(files.len(), 2);

    Ok(())
}

#[tokio::test]
async fn store_read_all_files() -> Result<()> {
    use chrono::*;

    let dir = tmpdir("store-read-all-files");
    prepare_files(dir.path()).await?;

    let time = Utc.with_ymd_and_hms(2026, 01, 01, 0, 0, 0).unwrap();
    let hour = 3600;
    let time_tz = time.with_timezone(&FixedOffset::east_opt(4 * hour).unwrap());

    let mut reader =
        EventReader::open(dir.path().to_path_buf(), &time_tz).await?;
    let mut events: Vec<TimedEvent> = vec![];

    while let Some(k) = reader.next_event().await? {
        events.push(k);
    }

    assert_eq!(events.len(), 4);

    let next1 = reader.next_event().await?;
    assert!(matches!(next1, None));

    let next2 = reader.next_event().await?;
    assert!(matches!(next2, None));

    // Should grab next events, if they become available.
    let path = dir.path().join("20260105.log");
    fs::write(
        &path,
        json_lines![
            {
                "timestamp": "2026-01-05T00:05:00Z",
                "event": {
                    "type": "awake"
                }
            }
        ],
    )
    .await?;

    let after1 = reader.next_event().await?;
    assert!(!matches!(after1, None));

    Ok(())
}

#[tokio::test]
async fn store_read_all_stream() -> Result<()> {
    use chrono::*;

    let dir = tmpdir("store-read-all-stream");
    prepare_files(dir.path()).await?;

    let time = Utc.with_ymd_and_hms(2026, 01, 01, 0, 0, 0).unwrap();
    let hour = 3600;
    let time_tz = time.with_timezone(&FixedOffset::east_opt(4 * hour).unwrap());

    let reader = EventReader::open(dir.path().to_path_buf(), &time_tz).await?;
    let stream = reader.into_stream();
    let collected: Vec<TimedEvent> = stream.try_collect().await?;

    assert_eq!(collected.len(), 4);

    Ok(())
}

#[tokio::test]
async fn store_read_all_stream_one_by_one() -> Result<()> {
    use chrono::*;

    let dir = tmpdir("store-read-all-stream-one-by-one");
    prepare_files(dir.path()).await?;

    let time = Utc.with_ymd_and_hms(2026, 01, 01, 0, 0, 0).unwrap();
    let hour = 3600;
    let time_tz = time.with_timezone(&FixedOffset::east_opt(4 * hour).unwrap());

    let reader = EventReader::open(dir.path().to_path_buf(), &time_tz).await?;
    let stream = reader.into_stream().fuse();

    futures::pin_mut!(stream);

    for i in 1..=4 {
        let item = stream.next().await;
        assert!(item.is_some(), "Expected item {i}");
        assert!(item.unwrap().is_ok(), "Expected item {i}");
    }

    // None
    assert!(stream.next().await.is_none());

    // Fused, stays None.
    assert!(stream.next().await.is_none());

    let mut vec = Vec::new();
    vec.push(&mut stream);

    // Fused, stays None.
    assert!(stream.next().await.is_none());

    Ok(())
}
