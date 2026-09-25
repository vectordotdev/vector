use std::{fs, io::Write, path::Path};

use async_compression::tokio::bufread::GzipEncoder;
use chrono::{DateTime, Utc};
use tokio::io::AsyncReadExt as _;

use bytes::Bytes;

use crate::file_watcher::FileWatcher;
use file_source_common::ReadFrom;

/// Test that get_bytes_unread() returns accurate values
#[tokio::test]
async fn test_bytes_unread_basic() {
    let dir = tempfile::TempDir::new().expect("could not create tempdir");
    let path = dir.path().join("test.log");

    // Create file with known content
    {
        let mut f = fs::File::create(&path).expect("could not create file");
        f.write_all(b"line1\nline2\nline3\n").unwrap();
        f.flush().unwrap();
    }

    // Create watcher starting from beginning
    let mut fw = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        100_000,
        Bytes::from("\n"),
    )
    .await
    .expect("could not create file watcher");

    // Initially, bytes_unread should be the full file size (18 bytes)
    assert_eq!(fw.get_bytes_unread().await, Some(18));

    // Read first line ("line1\n" = 6 bytes)
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // After reading 6 bytes, 12 bytes remain unread
    assert_eq!(fw.get_bytes_unread().await, Some(12));

    // Read second line ("line2\n" = 6 bytes)
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // After reading 12 bytes, 6 bytes remain unread
    assert_eq!(fw.get_bytes_unread().await, Some(6));

    // Read third line ("line3\n" = 6 bytes)
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // After reading all 18 bytes, 0 bytes remain
    assert_eq!(fw.get_bytes_unread().await, Some(0));
    assert!(fw.read_line().await.unwrap().raw_line.is_none());
    assert!(fw.reached_eof());
    assert_eq!(fw.get_unwatch_info().await.bytes_unread, Some(0));
}

/// Test that get_bytes_unread() still works after file is deleted
/// This is the key scenario for Kubernetes log rotation
#[cfg(unix)] // File deletion behavior differs on Windows
#[tokio::test]
async fn test_bytes_unread_after_delete() {
    let dir = tempfile::TempDir::new().expect("could not create tempdir");
    let path = dir.path().join("test.log");

    // Create file with known content
    {
        let mut f = fs::File::create(&path).expect("could not create file");
        f.write_all(b"line1\nline2\nline3\n").unwrap();
        f.flush().unwrap();
    }

    // Create watcher starting from beginning
    let mut fw = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        100_000,
        Bytes::from("\n"),
    )
    .await
    .expect("could not create file watcher");

    // Read first line
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    assert_eq!(fw.get_bytes_unread().await, Some(12));

    // Delete the file
    fs::remove_file(&path).expect("could not delete file");
    assert!(!path.exists());

    // bytes_unread should still work via the open file handle
    // Even though file is deleted, fd is still valid
    assert_eq!(fw.get_bytes_unread().await, Some(12));
}

/// Test that get_bytes_unread() tracks growing files correctly
#[tokio::test]
async fn test_bytes_unread_growing_file() {
    let dir = tempfile::TempDir::new().expect("could not create tempdir");
    let path = dir.path().join("test.log");

    // Create file with initial content
    let mut f = fs::File::create(&path).expect("could not create file");
    f.write_all(b"line1\n").unwrap();
    f.flush().unwrap();

    // Create watcher
    let mut fw = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        100_000,
        Bytes::from("\n"),
    )
    .await
    .expect("could not create file watcher");

    // Initial bytes_unread is 6
    assert_eq!(fw.get_bytes_unread().await, Some(6));

    // Append more content to the file
    f.write_all(b"line2\n").unwrap();
    f.flush().unwrap();

    // bytes_unread should now reflect the larger file size (12 bytes)
    // since we use current file size from metadata, not initial
    assert_eq!(fw.get_bytes_unread().await, Some(12));

    // Read first line
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // 12 - 6 = 6 bytes remaining
    assert_eq!(fw.get_bytes_unread().await, Some(6));
}

async fn write_gzip(path: &Path) {
    let mut compressed = Vec::new();
    GzipEncoder::new(&b"line1\nline2\n"[..])
        .read_to_end(&mut compressed)
        .await
        .unwrap();
    fs::write(path, compressed).unwrap();
}

#[tokio::test]
async fn test_bytes_unread_gzip_is_unknown_even_at_eof() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.gz");
    write_gzip(&path).await;
    let mut fw = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        100_000,
        Bytes::from_static(b"\n"),
    )
    .await
    .unwrap();

    assert_eq!(fw.get_bytes_unread().await, None);
    assert_eq!(fw.get_file_position(), 0);
    for expected in [b"line1", b"line2"] {
        let line = fw.read_line().await.unwrap().raw_line.unwrap();
        assert_eq!(line.bytes.as_ref(), expected);
        assert_eq!(fw.get_bytes_unread().await, None);
    }
    assert!(fw.read_line().await.unwrap().raw_line.is_none());
    assert!(fw.reached_eof());
    assert_eq!(fw.get_unwatch_info().await.bytes_unread, None);

    // Replacing the gzip reader preserves the old unknown count, while the
    // replacement plain reader can report the bytes beyond its inherited offset.
    let replacement = dir.path().join("replacement.log");
    fs::write(&replacement, b"line1\nline2\nline3\n").unwrap();
    let old_info = fw.update_path(replacement).await.unwrap().unwrap();
    assert_eq!(old_info.path, path);
    assert_eq!(old_info.bytes_unread, None);
    assert!(old_info.reached_eof);
    assert_eq!(fw.get_bytes_unread().await, Some(6));
}

#[tokio::test]
async fn test_bytes_unread_skipped_gzip_is_unknown() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("skipped.gz");
    write_gzip(&path).await;

    for (read_from, ignore_before) in [
        (ReadFrom::End, None),
        (ReadFrom::Checkpoint(6), None),
        (ReadFrom::Beginning, Some(DateTime::<Utc>::MAX_UTC)),
    ] {
        let mut fw = FileWatcher::new(
            path.clone(),
            read_from,
            ignore_before,
            100_000,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();

        assert_eq!(fw.get_bytes_unread().await, None);
        assert!(fw.read_line().await.unwrap().raw_line.is_none());
        let info = fw.get_unwatch_info().await;
        assert!(info.reached_eof);
        assert_eq!(info.bytes_unread, None);
    }
}
