use log::error;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

pub const LOCK_FILE_TIME_SEC: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct LockFile(std::fs::File);

impl Drop for LockFile {
    fn drop(&mut self) {
        if let Err(e) = self.0.unlock() {
            error!("Error unlocking file: {}", e);
        }
    }
}

#[derive(Debug, Error)]
pub enum LockFileError {
    #[error("LockFile IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("LockFile failed to acquire lock.")]
    TryLockError(#[from] std::fs::TryLockError),
}

pub async fn acquire_lock_file(
    filepath: PathBuf,
) -> Result<LockFile, LockFileError> {
    let filename = filepath.join("LOCK");

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(filename)
        .await
        .map_err(LockFileError::Io)?;

    let stdfile = file.into_std().await;
    stdfile.try_lock()?;
    let lock = LockFile(stdfile);

    Ok(lock)
}
