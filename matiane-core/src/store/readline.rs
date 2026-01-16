use crate::util::{memchr, memrchr};
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

// 512 KiB.
const DEFAULT_BUF_SIZE: NonZeroUsize = NonZeroUsize::new(512 * 1024).unwrap();
// 64 KiB.
const DEFAULT_REV_BUF_SIZE: NonZeroUsize =
    NonZeroUsize::new(64 * 1024).unwrap();

#[derive(Debug, Error)]
pub enum LineReaderError {
    #[error("Store IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("String UTF Error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

pub type ReaderResult<T> = Result<T, LineReaderError>;

pub trait LineReader {
    fn next_line(
        &mut self,
    ) -> impl Future<Output = ReaderResult<Option<String>>>;
    fn rewind(&mut self) -> impl Future<Output = ReaderResult<u64>>;
}

/// Reader reads buffer then processes, may not read full buffer.
pub struct FileLineReader {
    file: File,
    buffer: Buffer<Forward>,
    line_buf: Vec<u8>,
    eof: bool,
}

impl FileLineReader {
    pub fn new(file: File) -> Self {
        Self::with_buffer_size(file, DEFAULT_BUF_SIZE)
    }

    pub fn with_buffer_size(file: File, buffer_size: NonZeroUsize) -> Self {
        Self {
            file,
            buffer: Buffer::<Forward>::new(buffer_size),
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

impl LineReader for FileLineReader {
    async fn rewind(&mut self) -> ReaderResult<u64> {
        self.reset();
        Ok(self.file.seek(SeekFrom::Start(0)).await?)
    }

    async fn next_line(&mut self) -> ReaderResult<Option<String>> {
        while !self.eof {
            if self.buffer.unprocessed_len() == 0 {
                self.read_to_buffer().await?;
            }

            let unprocessed = self.buffer.unprocessed();

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

pub struct FileLineReverseReader {
    file: File,
    buffer: Buffer<Backward>,
    line_buf: Vec<u8>,
    done: bool,
    pos: u64,
}

impl FileLineReverseReader {
    pub fn new(file: File) -> Self {
        Self::with_buffer_size(file, DEFAULT_REV_BUF_SIZE)
    }

    pub fn with_buffer_size(file: File, buffer_size: NonZeroUsize) -> Self {
        Self {
            file,
            buffer: Buffer::<Backward>::new(buffer_size),
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

impl LineReader for FileLineReverseReader {
    async fn rewind(&mut self) -> ReaderResult<u64> {
        self.reset();
        self.pos = self.file.seek(SeekFrom::End(0)).await?;

        Ok(self.pos)
    }

    async fn next_line(&mut self) -> ReaderResult<Option<String>> {
        loop {
            let process = self.buffer.unprocessed();

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

#[derive(Debug)]
struct Forward;

#[derive(Debug)]
struct Backward;

#[derive(Debug)]
struct Buffer<D> {
    processed: usize,
    filled: usize,
    size: usize,
    data: Vec<u8>,
    _dir: PhantomData<D>,
}

impl<D> Buffer<D> {
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

    fn reset(&mut self) {
        self.filled = 0;
        self.processed = 0;
    }
}

impl Buffer<Forward> {
    fn new(size: NonZeroUsize) -> Self {
        Self {
            filled: 0,
            processed: 0,
            size: size.get(),
            data: vec![0; size.get()],
            _dir: PhantomData,
        }
    }

    fn unprocessed(&self) -> &[u8] {
        &self.data[self.processed..self.filled]
    }
}

impl Buffer<Backward> {
    fn new(size: NonZeroUsize) -> Self {
        Self {
            filled: 0,
            processed: 0,
            size: size.get(),
            data: vec![0; size.get()],
            _dir: PhantomData,
        }
    }

    fn unprocessed(&self) -> &[u8] {
        &self.data[0..(self.filled - self.processed)]
    }
}
