use std::{fs, io::Write};

use bytes::Bytes;

use crate::file_watcher::FileWatcher;
use file_source_common::ReadFrom;

/// Test that get_bytes_dropped() returns accurate values
#[tokio::test]
async fn test_bytes_dropped_basic() {
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

    // Initially, bytes_dropped should be the full file size (18 bytes)
    assert_eq!(fw.get_bytes_dropped().await, 18);

    // Read first line ("line1\n" = 6 bytes)
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // After reading 6 bytes, 12 bytes remain unread
    assert_eq!(fw.get_bytes_dropped().await, 12);

    // Read second line ("line2\n" = 6 bytes)
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // After reading 12 bytes, 6 bytes remain unread
    assert_eq!(fw.get_bytes_dropped().await, 6);

    // Read third line ("line3\n" = 6 bytes)
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // After reading all 18 bytes, 0 bytes remain
    assert_eq!(fw.get_bytes_dropped().await, 0);
}

/// Test that get_bytes_dropped() still works after file is deleted
/// This is the key scenario for Kubernetes log rotation
#[cfg(unix)] // File deletion behavior differs on Windows
#[tokio::test]
async fn test_bytes_dropped_after_delete() {
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
    assert_eq!(fw.get_bytes_dropped().await, 12);

    // Delete the file
    fs::remove_file(&path).expect("could not delete file");
    assert!(!path.exists());

    // bytes_dropped should still work via the open file handle
    // Even though file is deleted, fd is still valid
    assert_eq!(fw.get_bytes_dropped().await, 12);
}

/// Test that get_bytes_dropped() tracks growing files correctly
#[tokio::test]
async fn test_bytes_dropped_growing_file() {
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

    // Initial bytes_dropped is 6
    assert_eq!(fw.get_bytes_dropped().await, 6);

    // Append more content to the file
    f.write_all(b"line2\n").unwrap();
    f.flush().unwrap();

    // bytes_dropped should now reflect the larger file size (12 bytes)
    // since we use current file size from metadata, not initial
    assert_eq!(fw.get_bytes_dropped().await, 12);

    // Read first line
    let result = fw.read_line().await.expect("read failed");
    assert!(result.raw_line.is_some());
    // 12 - 6 = 6 bytes remaining
    assert_eq!(fw.get_bytes_dropped().await, 6);
}
