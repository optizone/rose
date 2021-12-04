use std::{
    ops::DerefMut,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use either::Either;
use thiserror::Error;
use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncRead, AsyncWriteExt, BufReader, BufWriter, ReadBuf},
};
use uuid::Uuid;

mod cache;
use cache::*;

#[derive(Clone)]
pub struct ImageManager {
    path: PathBuf,
    cache: Cache,
}

pub struct CachedImage {
    data: Arc<Vec<u8>>,
    pos: usize,
}

impl AsyncRead for CachedImage {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let rem = &self.data.as_slice()[self.pos..];
        let amt = std::cmp::min(buf.remaining(), self.data.len() - self.pos);
        let put = &rem[..amt];
        buf.put_slice(put);
        self.deref_mut().pos += amt;
        Poll::Ready(Ok(()))
    }
}

impl From<Arc<Vec<u8>>> for CachedImage {
    fn from(cached: Arc<Vec<u8>>) -> Self {
        Self {
            data: cached,
            pos: 0,
        }
    }
}

impl ImageManager {
    pub fn new() -> Self {
        let path = std::env::var("IMAGES_DIR").unwrap();
        let cache = Cache::new(Duration::from_secs(1));

        Self {
            path: path.into(),
            cache,
        }
    }

    #[inline]
    fn uuid_to_path(&self, uuid: Uuid) -> PathBuf {
        let (_, _, _, id) = uuid.to_fields_le();
        let dir1 = format!("{:x}", id[2]);
        let dir2 = format!("{:x}", id[3]);

        self.path
            .join(dir1)
            .join(dir2)
            .join(format!("{}.jpeg", uuid.to_string()))
    }

    pub async fn get(&self, uuid: Uuid) -> Result<Either<BufReader<File>, CachedImage>, Error> {
        if let Some(cached) = self.cache.get(uuid) {
            return Ok(Either::Right(cached.into()));
        }

        let path = self.uuid_to_path(uuid);
        let file = OpenOptions::new().read(true).open(path).await?;

        let reader = BufReader::new(file);

        Ok(Either::Left(reader))
    }

    pub async fn insert(&self, uuid: Uuid, data: Vec<u8>) -> Result<(), Error> {
        let path = self.uuid_to_path(uuid);
        let dir = path.ancestors().nth(1).unwrap();

        if tokio::fs::metadata(dir).await.is_err() {
            tokio::fs::create_dir_all(dir).await?;
        }

        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .await?;

        let mut writer = BufWriter::new(file);
        writer.write_all(&data).await?;

        self.cache.insert(uuid, Arc::new(data));

        Ok(())
    }

    pub async fn load_cache(&self, uuids: Vec<Uuid>) -> Result<(), Error> {
        for uuid in uuids {
            self.get(uuid).await?;
        }
        self.cache.force_update();
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] tokio::io::Error),
}
