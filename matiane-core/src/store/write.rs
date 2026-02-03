use thiserror::Error;

use super::filepath::Filepath;
use crate::events::TimedEvent;
use chrono::{DateTime, NaiveDate, Utc};
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
    file: File,
    file_path: Filepath,
}

impl EventWriter {
    pub async fn open(
        dir: PathBuf,
        date: DateTime<Utc>,
    ) -> Result<Self, StoreWriteError> {
        let dir_exists = tokio::fs::try_exists(&dir).await?;

        if !dir_exists {
            tokio::fs::create_dir(&dir).await?;
        }

        let filepath = Into::<Filepath>::into(date).with_path(dir);

        log::debug!("opening log file: {:?}", filepath);

        let file = open_write_file(filepath.to_path_buf()).await?;

        let store = EventWriter {
            file,
            file_path: filepath,
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
        Ok(self.file.flush().await?)
    }

    pub async fn maybe_rotate(
        &mut self,
        date: NaiveDate,
    ) -> Result<(), StoreWriteError> {
        if self.file_path.date() == &date {
            return Ok(());
        }

        self.file_path.set_date(date);

        log::debug!("Rotating file: {:?}", self.file_path);
        let file = open_write_file(self.file_path.to_path_buf()).await?;

        self.flush().await?;

        self.file = file;

        Ok(())
    }
}

async fn open_write_file(filepath: PathBuf) -> Result<File, StoreWriteError> {
    Ok(tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filepath)
        .await?)
}
