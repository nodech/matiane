use thiserror::Error;

use crate::events::TimedEvent;
use chrono::{DateTime, NaiveDate, Utc};
use log::error;
use serde_json;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum StoreWriteError {
    #[error("Store IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Store failed to encode event")]
    EncodeError(#[from] serde_json::Error),
}

pub struct EventWriter {
    dir: PathBuf,
    file: File,
    current_date: NaiveDate,
}

impl EventWriter {
    pub async fn open(
        dir: PathBuf,
        date: DateTime<Utc>,
    ) -> Result<Self, StoreWriteError> {
        let filename = get_filename_by_date(date.date_naive());
        let filepath = dir.join(filename);

        let dir_exists = tokio::fs::try_exists(&dir).await?;

        if !dir_exists {
            tokio::fs::create_dir(&dir).await?;
        }

        log::debug!("opening log file: {:?}", filepath);
        let file = open_write_file(filepath).await?;

        let store = EventWriter {
            dir,
            file,
            current_date: date.date_naive(),
        };

        Ok(store)
    }

    pub async fn write(
        &mut self,
        event: &TimedEvent,
    ) -> Result<(), StoreWriteError> {
        self.maybe_rotate(event.timestamp.date_naive()).await?;

        let mut encoded = serde_json::to_vec(&event)?;
        encoded.push(b'\n');

        self.file.write_all(&encoded).await?;

        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), StoreWriteError> {
        self.file.flush().await.map_err(StoreWriteError::Io)
    }

    pub async fn maybe_rotate(
        &mut self,
        date: NaiveDate,
    ) -> Result<(), StoreWriteError> {
        if self.current_date == date {
            return Ok(());
        }

        let filename = get_filename_by_date(date);
        let filepath = self.dir.join(filename);
        log::debug!("Rotating file: {:?}", filepath);
        let file = open_write_file(filepath).await?;

        self.flush().await?;

        self.file = file;
        self.current_date = date;

        Ok(())
    }
}

async fn open_write_file(filepath: PathBuf) -> Result<File, StoreWriteError> {
    tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filepath)
        .await
        .map_err(StoreWriteError::Io)
}

fn get_filename_by_date(date: NaiveDate) -> PathBuf {
    PathBuf::from(date.format("%Y%m%d").to_string()).with_extension("log")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use chrono::TimeZone;

    #[test]
    fn get_filename_by_name_test() -> Result<()> {
        struct TestCase {
            date: DateTime<Utc>,
            expected: PathBuf,
        }

        let dates = [
            TestCase {
                date: Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap(),
                expected: "19700101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(1970, 1, 1, 23, 59, 59).unwrap(),
                expected: "19700101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                expected: "20000101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(2000, 1, 1, 5, 6, 7).unwrap(),
                expected: "20000101.log".into(),
            },
            TestCase {
                date: Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap(),
                expected: "20251231.log".into(),
            },
        ];

        for test in dates {
            let name = get_filename_by_date(test.date.date_naive());
            assert_eq!(name, test.expected);
        }

        Ok(())
    }
}
