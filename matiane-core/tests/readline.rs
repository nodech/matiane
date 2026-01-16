use anyhow::Result;
use matiane_core::store::readline::{
    FileLineReader, FileLineReverseReader, LineReader,
};
use std::num::NonZeroUsize;
use tempfile::{Builder, TempDir};
use tokio::fs::{self, File};

fn tmpdir(name: &str) -> TempDir {
    Builder::new()
        .prefix(&format!("matiane-core-{}", name))
        .rand_bytes(10)
        .tempdir()
        .unwrap()
}

async fn setup_file(contents: &str) -> Result<(TempDir, File)> {
    let dir = tmpdir("test-dir");
    let filepath = dir.path().join("filename.log");

    fs::write(&filepath, &contents).await?;

    let file = fs::File::open(&filepath).await?;

    Ok((dir, file))
}

async fn get_lines(lines: &[&str], buffer_size: usize) -> Result<Vec<String>> {
    assert!(buffer_size > 0);

    let content = lines.join("\n");
    let (_dir, file) = setup_file(&content).await?;

    let mut reader = FileLineReader::with_buffer_size(
        file,
        NonZeroUsize::new(buffer_size).unwrap(),
    );

    let mut lines = Vec::new();

    while let Some(line) = reader.next_line().await? {
        lines.push(line);
    }

    return Ok(lines);
}

async fn get_lines_backward(
    lines: &[&str],
    buffer_size: usize,
) -> Result<Vec<String>> {
    assert!(buffer_size > 0);
    let content = lines.join("\n");
    let (_dir, file) = setup_file(&content).await?;

    let mut reader = FileLineReverseReader::with_buffer_size(
        file,
        NonZeroUsize::new(buffer_size).unwrap(),
    );

    reader.rewind().await?;

    let mut lines = Vec::new();

    while let Some(line) = reader.next_line().await? {
        lines.push(line);
    }

    return Ok(lines);
}

#[tokio::test]
async fn readline_small_forward() -> Result<()> {
    let expected_lines = vec!["Line 1", "Line 2", "Line 3"];
    let lines = get_lines(&expected_lines, 100).await?;
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_small_backward() -> Result<()> {
    let mut expected_lines = vec!["Line 1", "Line 2", "Line 3"];
    let lines = get_lines_backward(&expected_lines, 100).await?;

    expected_lines.reverse();
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_buffered_success_forward() -> Result<()> {
    let expected_lines = vec!["Line 1", "Line 2", "Line 3"];
    let lines = get_lines(&expected_lines, 4).await?;
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_buffered_success_backward() -> Result<()> {
    let mut expected_lines = vec!["Line 1", "Line 2", "Line 3"];
    let lines = get_lines_backward(&expected_lines, 4).await?;

    expected_lines.reverse();
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_buffer_too_small_forward() -> Result<()> {
    let expected_lines = vec!["Line 1", "Line 2", "Line 3"];
    let lines = get_lines(&expected_lines, 1).await?;

    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_buffer_too_small_backward() -> Result<()> {
    let mut expected_lines = vec!["Line 1", "Line 2", "Line 3"];
    let lines = get_lines_backward(&expected_lines, 1).await?;

    expected_lines.reverse();
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_buffer_empty_last_forward() -> Result<()> {
    let expected_lines = vec!["Line 1", "Line 2", "Line 3", ""];
    let lines = get_lines(&expected_lines, 1).await?;

    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_buffer_empty_last_backward() -> Result<()> {
    let mut expected_lines = vec!["Line 1", "Line 2", "Line 3", ""];
    let lines = get_lines_backward(&expected_lines, 1).await?;

    expected_lines.reverse();
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_just_eof_forward() -> Result<()> {
    let expected_lines = vec!["Line 1"];
    let lines = get_lines(&expected_lines, 10).await?;
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_just_eof_backward() -> Result<()> {
    let mut expected_lines = vec!["Line 1"];
    let lines = get_lines_backward(&expected_lines, 10).await?;

    expected_lines.reverse();
    assert_eq!(lines, expected_lines);

    Ok(())
}

#[tokio::test]
async fn readline_rewind_forward() -> Result<()> {
    let lines = ["Line 1", "Line 2", "Line 3"];
    let content = lines.join("\n");
    let (_dir, file) = setup_file(&content).await?;

    let mut reader =
        FileLineReader::with_buffer_size(file, NonZeroUsize::new(100).unwrap());

    assert_eq!(reader.next_line().await?, Some("Line 1".into()));
    assert_eq!(reader.next_line().await?, Some("Line 2".into()));

    let seek_res = reader.rewind().await?;

    assert_eq!(seek_res, 0);
    assert_eq!(reader.next_line().await?, Some("Line 1".into()));

    return Ok(());
}

#[tokio::test]
async fn readline_rewind_backward() -> Result<()> {
    let lines = ["Line 1", "Line 2", "Line 3"];
    let content = lines.join("\n");
    let (_dir, file) = setup_file(&content).await?;

    let mut reader = FileLineReverseReader::with_buffer_size(
        file,
        NonZeroUsize::new(100).unwrap(),
    );

    reader.rewind().await?;

    assert_eq!(reader.next_line().await?, Some("Line 3".into()));
    assert_eq!(reader.next_line().await?, Some("Line 2".into()));

    let seek_res = reader.rewind().await?;

    assert_eq!(seek_res, 20);
    assert_eq!(reader.next_line().await?, Some("Line 3".into()));

    return Ok(());
}
