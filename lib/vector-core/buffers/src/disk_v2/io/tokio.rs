use std::{path::PathBuf, io};

use async_trait::async_trait;
use tokio::fs::{File, OpenOptions, self};

use super::Filesystem;

pub struct TokioFilesystem;

#[async_trait]
impl Filesystem for TokioFilesystem {
    type File = File;

    async fn open_writable(&self, path: &PathBuf) -> io::Result<Self::File> {
        OpenOptions::new()
			.read(true)
			.write(true)
			.create(true)
			.open(path)
			.await
    }

    async fn open_writable_atomic(&self, path: &PathBuf) -> io::Result<Self::File> {
        OpenOptions::new()
			.read(true)
			.write(true)
			.create_new(true)
			.open(path)
			.await
    }

    async fn open_readable(&self, path: &PathBuf) -> io::Result<Self::File> {
        OpenOptions::new()
			.read(true)
			.open(path)
			.await
    }

    async fn delete(&self, path: &PathBuf) -> io::Result<()> {
        fs::remove_file(path).await
    }
}