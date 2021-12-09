use std::{
    path::{Path, PathBuf},
    pin::Pin,
};

use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncRead, AsyncWrite},
};

pub struct AtomicFile {
    file: File,
    path_temp: PathBuf,
    path: PathBuf,
    finalized: bool,
}

impl AtomicFile {
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, io::Error> {
        let path_temp = format!("{}.temp", path.as_ref().to_string_lossy()).into();
        let path = path.as_ref().into();
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path_temp)
            .await?;
        Ok(Self {
            path_temp,
            path,
            file,
            finalized: false,
        })
    }

    pub async fn finalize(&mut self) -> Result<(), io::Error> {
        self.finalized = true;
        if !self.finalized {
            tokio::fs::rename(&self.path_temp, &self.path).await?;
        }
        Ok(())
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        if !self.finalized {
            let path = self.path_temp.clone();
            actix_rt::spawn(async move { tokio::fs::remove_file(path).await });
        }
    }
}

impl AsyncWrite for AtomicFile {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let file = Pin::new(&mut self.file);
        file.poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let file = Pin::new(&mut self.file);
        file.poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let file = Pin::new(&mut self.file);
        file.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let file = Pin::new(&mut self.file);
        file.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.file.is_write_vectored()
    }
}

impl AsyncRead for AtomicFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let file = Pin::new(&mut self.file);
        file.poll_read(cx, buf)
    }
}
