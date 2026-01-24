use thiserror::Error;

use super::filepath::Filepath;
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
        let dir_exists = tokio::fs::try_exists(&dir).await?;

        if !dir_exists {
            tokio::fs::create_dir(&dir).await?;
        }

        let mut filepath: Filepath = date.into();
        filepath.set_path(dir.clone());

        log::debug!("opening log file: {:?}", filepath);

        let file = open_write_file(filepath.into()).await?;

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
        Ok(self.file.flush().await?)
    }

    pub async fn maybe_rotate(
        &mut self,
        date: NaiveDate,
    ) -> Result<(), StoreWriteError> {
        if self.current_date == date {
            return Ok(());
        }

        let mut filepath = Into::<Filepath>::into(date);
        filepath.set_path(self.dir.clone());

        log::debug!("Rotating file: {:?}", filepath);
        let file = open_write_file(filepath.into()).await?;

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
