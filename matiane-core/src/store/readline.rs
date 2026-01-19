use crate::util::{memchr, memrchr};
use futures::stream::{self, Stream};
use std::cmp::Ordering;
use std::num::NonZeroUsize;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

// 512 KiB.
const DEFAULT_BUF_SIZE: NonZeroUsize = NonZeroUsize::new(512 * 1024).unwrap();

// 64 KiB.
const DEFAULT_REV_BUF_SIZE: NonZeroUsize =
    NonZeroUsize::new(64 * 1024).unwrap();

// 4 KiB.
const DEFAULT_SEEK_BUF_SIZE: NonZeroUsize =
    NonZeroUsize::new(4 * 1024).unwrap();

#[derive(Debug, Error)]
pub enum LineReaderError {
    #[error("Store IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("String UTF Error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("Compare Error: {0}")]
    Compare(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl LineReaderError {
    pub fn compare<E: std::error::Error + Send + Sync + 'static>(e: E) -> Self {
        Self::Compare(Box::new(e))
    }
}

pub type ReaderResult<T> = Result<T, LineReaderError>;

pub trait LineReader {
    fn next_line(
        &mut self,
    ) -> impl Future<Output = ReaderResult<Option<String>>>;

    fn rewind(&mut self) -> impl Future<Output = ReaderResult<u64>>;

    fn seek(
        &mut self,
        pos: SeekFrom,
    ) -> impl Future<Output = ReaderResult<u64>>;

    fn into_stream(self) -> impl Stream<Item = ReaderResult<String>>
    where
        Self: Sized,
    {
        stream::unfold(self, |mut reader| async {
            match reader.next_line().await {
                Ok(Some(line)) => Some((Ok(line), reader)),
                Ok(None) => None,
                Err(e) => Some((Err(e), reader)),
            }
        })
    }
}

/// Reader reads buffer then processes, may not read full buffer.
pub struct FileLineReader<'a> {
    file: &'a mut File,
    buffer: Buffer,
    line_buf: Vec<u8>,
    eof: bool,
}

impl<'a> FileLineReader<'a> {
    pub fn new(file: &'a mut File) -> Self {
        Self::with_buffer_size(file, DEFAULT_BUF_SIZE)
    }

    pub fn with_buffer_size(
        file: &'a mut File,
        buffer_size: NonZeroUsize,
    ) -> Self {
        Self {
            file,
            buffer: Buffer::new(buffer_size),
            line_buf: Vec::new(),
            eof: false,
        }
    }

    fn reset(&mut self) {
        self.line_buf.clear();
        self.buffer.reset();
        self.eof = false;
    }

    async fn read_to_buffer(&mut self) -> ReaderResult<()> {
        let buf = self.buffer.unfilled_mut();
        let read_bytes = self.file.read(buf).await?;

        self.buffer.advance_filled(read_bytes);

        if read_bytes == 0 {
            self.eof = true;
        }

        Ok(())
    }
}

impl LineReader for FileLineReader<'_> {
    async fn rewind(&mut self) -> ReaderResult<u64> {
        self.seek(SeekFrom::Start(0)).await
    }

    async fn seek(&mut self, pos: SeekFrom) -> ReaderResult<u64> {
        self.reset();
        Ok(self.file.seek(pos).await?)
    }

    async fn next_line(&mut self) -> ReaderResult<Option<String>> {
        while !self.eof {
            if self.buffer.unprocessed_len() == 0 {
                self.read_to_buffer().await?;
            }

            let unprocessed = self.buffer.unprocessed_forward();

            if let Some(n) = memchr(b'\n', unprocessed) {
                self.line_buf.extend_from_slice(&unprocessed[0..n]);

                let raw_line = std::mem::take(&mut self.line_buf);
                let line = String::from_utf8(raw_line)?;
                self.buffer.advance_processed(n + 1);

                if self.buffer.unprocessed_len() == 0 {
                    self.buffer.reset()
                }

                return Ok(Some(line));
            }

            self.line_buf.extend_from_slice(unprocessed);
            self.buffer.reset();

            if self.eof {
                let raw_line = std::mem::take(&mut self.line_buf);
                let line = String::from_utf8(raw_line)?;
                return Ok(Some(line));
            }
        }

        Ok(None)
    }
}

#[derive(Debug)]
pub struct FileLineReverseReader<'a> {
    file: &'a mut File,
    buffer: Buffer,
    line_buf: Vec<u8>,
    done: bool,
    pos: u64,
}

impl<'a> FileLineReverseReader<'a> {
    pub fn new(file: &'a mut File) -> Self {
        Self::with_buffer_size(file, DEFAULT_REV_BUF_SIZE)
    }

    pub fn with_buffer_size(
        file: &'a mut File,
        buffer_size: NonZeroUsize,
    ) -> Self {
        Self {
            file,
            buffer: Buffer::new(buffer_size),
            line_buf: Vec::new(),
            done: false,
            pos: 0,
        }
    }

    fn reset(&mut self) {
        self.line_buf.clear();
        self.buffer.reset();
        self.done = false;
    }

    pub async fn fill_buffer(&mut self) -> ReaderResult<()> {
        self.buffer.reset();

        let read_size = self.pos.min(self.buffer.capacity() as u64);

        if read_size == 0 {
            self.done = true;
            return Ok(());
        }

        self.pos -= read_size;
        self.file.seek(SeekFrom::Start(self.pos)).await?;

        let mut remaining = read_size as usize;
        while remaining > 0 {
            let buf = &mut self.buffer.unfilled_mut()[..remaining];
            let read = self.file.read(buf).await?;
            self.buffer.advance_filled(read);

            remaining -= read;
        }

        Ok(())
    }
}

impl LineReader for FileLineReverseReader<'_> {
    async fn rewind(&mut self) -> ReaderResult<u64> {
        self.seek(SeekFrom::End(0)).await
    }

    async fn seek(&mut self, pos: SeekFrom) -> ReaderResult<u64> {
        self.reset();
        self.pos = self.file.seek(pos).await?;

        Ok(self.pos)
    }

    async fn next_line(&mut self) -> ReaderResult<Option<String>> {
        loop {
            let process = self.buffer.unprocessed_backward();

            if self.done && process.is_empty() {
                if !self.line_buf.is_empty() {
                    let line =
                        String::from_utf8(std::mem::take(&mut self.line_buf))?;
                    return Ok(Some(line));
                }

                return Ok(None);
            }

            if let Some(n) = memrchr(b'\n', process) {
                let prefix = &process[n + 1..];
                let line = String::from_utf8(concat_slices(
                    prefix,
                    &std::mem::take(&mut self.line_buf),
                ))?;
                self.buffer.advance_processed(prefix.len() + 1);

                return Ok(Some(line));
            } else {
                self.line_buf = concat_slices(process, &self.line_buf);
                self.fill_buffer().await?;
            }
        }
    }
}

#[inline]
fn concat_slices(pre: &[u8], post: &[u8]) -> Vec<u8> {
    let mut concatted = Vec::with_capacity(pre.len() + post.len());
    concatted.extend_from_slice(pre);
    concatted.extend_from_slice(post);
    concatted
}

/// Binary search line in the file with custom comparator.
pub struct BinarySearch<'a, F>
where
    F: Fn(&str) -> ReaderResult<Ordering>,
{
    file: &'a mut File,
    cmp: F,
    buffer_size: NonZeroUsize,
}

impl<'a, F> BinarySearch<'a, F>
where
    F: Fn(&str) -> ReaderResult<Ordering>,
{
    pub fn new(file: &'a mut File, cmp: F) -> Self {
        Self {
            file,
            cmp,
            buffer_size: DEFAULT_SEEK_BUF_SIZE,
        }
    }

    pub fn buffer_size(mut self, size: NonZeroUsize) -> Self {
        self.buffer_size = size;
        self
    }

    pub async fn seek(mut self) -> ReaderResult<Option<u64>> {
        let fmeta = self.file.metadata().await?;

        let file_len = fmeta.len();

        if file_len == 0 {
            return Ok(None);
        }

        let mut left: u64 = 0;
        let mut right: u64 = fmeta.len();

        loop {
            let mid = (right + left) / 2;

            if left >= right {
                return Ok(Some(mid + 1));
            }

            let Some(line_start) = self.line_start(mid).await? else {
                return Ok(None);
            };

            let line = {
                let mut forwards = FileLineReader::with_buffer_size(
                    &mut self.file,
                    self.buffer_size,
                );

                forwards.seek(SeekFrom::Start(line_start)).await?;
                forwards.next_line().await?
            };

            let Some(line) = line else {
                return Ok(None);
            };

            match (self.cmp)(&line)? {
                Ordering::Less => {
                    left = line_start + (line.len() as u64) + 1;

                    if left > file_len {
                        return Ok(None);
                    }
                }
                Ordering::Equal => {
                    return Ok(Some(line_start));
                }
                Ordering::Greater => {
                    if line_start == 0 {
                        return Ok(None);
                    }

                    right = line_start - 1;
                }
            }
        }
    }

    async fn line_start(&mut self, pos: u64) -> ReaderResult<Option<u64>> {
        let mut backwards = FileLineReverseReader::with_buffer_size(
            &mut self.file,
            self.buffer_size,
        );

        backwards.seek(SeekFrom::Start(pos)).await?;

        if let Some(partial) = backwards.next_line().await? {
            Ok(Some(pos.saturating_sub(partial.len() as u64)))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
struct Buffer {
    processed: usize,
    filled: usize,
    size: usize,
    data: Vec<u8>,
}

impl Buffer {
    fn new(size: NonZeroUsize) -> Self {
        Self {
            filled: 0,
            processed: 0,
            size: size.get(),
            data: vec![0; size.get()],
        }
    }

    fn capacity(&self) -> usize {
        self.size
    }

    fn advance_filled(&mut self, n: usize) {
        self.filled += n;
    }

    fn unfilled_mut(&mut self) -> &mut [u8] {
        &mut self.data[self.filled..]
    }

    fn advance_processed(&mut self, n: usize) {
        self.processed += n;
    }

    fn unprocessed_len(&self) -> usize {
        self.filled - self.processed
    }

    fn unprocessed_forward(&self) -> &[u8] {
        &self.data[self.processed..self.filled]
    }

    fn unprocessed_backward(&self) -> &[u8] {
        &self.data[0..(self.filled - self.processed)]
    }

    fn reset(&mut self) {
        self.filled = 0;
        self.processed = 0;
    }
}
