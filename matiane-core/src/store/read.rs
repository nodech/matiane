use super::filepath::{Filepath, TryIntoFilenameError};
use super::readline::{AsyncLineReader, FileLineReaderOwned, LineReaderError};
use crate::events::TimedEvent;
use crate::store::readline::LineReader;
use chrono::{DateTime, FixedOffset};
use futures::stream::{self, Stream};
use futures::{StreamExt, TryStreamExt};
use serde_json;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs::{self, File};
use tokio_stream::wrappers::ReadDirStream;

#[derive(Debug, Error)]
pub enum StoreReadError {
    #[error("Store IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Store failed to decode an event")]
    EncodeError(#[from] serde_json::Error),
    #[error("Filepath error: {0}")]
    FilePathError(#[from] TryIntoFilenameError),
    #[error("Could not find in the range")]
    NoFilesToOpen,
    #[error("Failed to read line: {0}")]
    LineReaderError(#[from] LineReaderError),
}

pub type EventReaderResult<T> = Result<T, StoreReadError>;

pub struct EventReader {
    file_path: Filepath,
    line_reader: FileLineReaderOwned,
}

impl EventReader {
    pub async fn open(
        dir: PathBuf,
        open_at: &DateTime<FixedOffset>,
    ) -> EventReaderResult<Self> {
        let utc_naive = open_at.to_utc().date_naive();

        let from_path =
            Into::<Filepath>::into(utc_naive).with_path(dir.clone());

        let first = {
            let entries = Self::list_files(&dir).await?;
            entries.range(&from_path..).next().cloned()
        }
        .ok_or(StoreReadError::NoFilesToOpen)?;

        let path = first.to_path_buf();
        log::debug!("Opening file: {:?}", &path);
        let file = open_read_file(&path).await?;

        Ok(Self {
            file_path: first,
            line_reader: AsyncLineReader::new(file),
        })
    }

    pub async fn list_files(dir: &Path) -> EventReaderResult<StoreDirectory> {
        ReadDirStream::new(fs::read_dir(dir).await?)
            .map_err(StoreReadError::Io)
            .filter_map(async |rde| {
                // We don't care about the Filepath parsing errors.
                // If the file in the directory fails parsing then just skip it.
                rde.map(|e| e.path().try_into().ok()).transpose()
            })
            .try_collect()
            .await
    }

    pub async fn next_event(
        &mut self,
    ) -> EventReaderResult<Option<TimedEvent>> {
        let line = loop {
            if let Some(l) = self.line_reader.next_line().await? {
                break l;
            }

            if !self.open_next_file().await? {
                return Ok(None);
            }
        };

        Ok(serde_json::from_str(&line)?)
    }

    pub async fn open_next_file(&mut self) -> EventReaderResult<bool> {
        let mut next_file = self.file_path.clone();
        next_file.increment_date();

        let next_fp = match Self::list_files(self.file_path.path()).await {
            Ok(dir) => dir.range(&next_file..).next().cloned(),
            Err(err) => return Err(err),
        };

        match next_fp {
            Some(fp) => {
                let path = fp.to_path_buf();
                log::debug!("Opening next file: {:?}", path);
                let file = open_read_file(&path).await?;

                self.line_reader = AsyncLineReader::new(file);
                self.file_path = fp;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn into_stream(
        self,
    ) -> impl Stream<Item = EventReaderResult<TimedEvent>>
    where
        Self: Sized,
    {
        stream::unfold(self, |mut reader| async {
            match reader.next_event().await {
                Ok(Some(line)) => Some((Ok(line), reader)),
                Ok(None) => None,
                Err(e) => Some((Err(e), reader)),
            }
        })
    }
}

#[derive(Debug, Default)]
pub struct StoreDirectory {
    pub items: BTreeSet<Filepath>,
}

impl StoreDirectory {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn range(
        &self,
        range: impl std::ops::RangeBounds<Filepath>,
    ) -> std::collections::btree_set::Range<'_, Filepath> {
        self.items.range(range)
    }
}

impl std::iter::Extend<Filepath> for StoreDirectory {
    #[inline]
    fn extend<Iter: IntoIterator<Item = Filepath>>(&mut self, iter: Iter) {
        self.items.extend(iter);
    }
}

async fn open_read_file(filepath: &PathBuf) -> EventReaderResult<File> {
    Ok(tokio::fs::OpenOptions::new()
        .read(true)
        .open(filepath)
        .await?)
}
