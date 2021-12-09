use std::io::Write;
use std::{
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use crate::error::Error;
use actix_web::web::Bytes;
use futures_util::{Stream, StreamExt};
use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, ReadBuf},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

mod cache;
use cache::*;

mod atomic_file;
use atomic_file::*;

#[derive(Clone)]
#[allow(dead_code)]
pub struct FsManager {
    dir: PathBuf,
    cache: Cache,
    max_cache_size: u64,
    max_cache_entry_size: u64,
}

pub struct Data {
    source: DataSource,
    uuid: Uuid,
    pos: usize,
}

enum DataSource {
    Reader {
        reader: BufReader<File>,
        buf: Option<Vec<u8>>,
        cache: Option<Cache>,
    },
    Cache(Arc<Vec<u8>>),
}

impl FsManager {
    pub fn from_env() -> Self {
        let path = std::env::var("ROSE_FILES_DIR").unwrap().into();
        let cache = Cache::new(Duration::from_secs(1));
        let max_cache_size = std::env::var("ROSE_MAX_CACHE_SIZE")
            .unwrap()
            .parse()
            .expect("ROSE_MAX_CACHE_SIZE as an unsigned integer");
        let max_cache_entry_size = std::env::var("ROSE_MAX_CACHE_ENTRY_SIZE")
            .unwrap()
            .parse()
            .expect("ROSE_MAX_CACHE_ENTRY_SIZE as an unsigned integer");

        Self {
            dir: path,
            cache,
            max_cache_size,
            max_cache_entry_size,
        }
    }

    #[inline]
    fn uuid_to_path(&self, uuid: Uuid, file_ext: &str) -> Result<PathBuf, std::io::Error> {
        let mut s = String::with_capacity(256);
        let (_, _, _, id) = uuid.to_fields_le();

        write!(
            unsafe { s.as_mut_vec() },
            "{dir}{sep}{id0}{sep}{id1}{sep}{uuid}.{ext}",
            dir = self.dir.as_path().display(),
            sep = std::path::MAIN_SEPARATOR,
            id0 = id[2],
            id1 = id[3],
            uuid = uuid,
            ext = file_ext
        )?;

        Ok(PathBuf::from(s))
    }

    pub async fn get(
        &self,
        uuid: Uuid,
        file_ext: &str,
    ) -> Result<impl Stream<Item = Result<Bytes, Error>>, Error> {
        let map = |r: Result<_, _>| r.map_err(|e| Error::from(e));
        if let Some(cached) = self.cache.get(&uuid) {
            return Ok(ReaderStream::new(Data::from(cached)).map(map));
        }

        let path = self.uuid_to_path(uuid, file_ext)?;
        let file = OpenOptions::new().read(true).open(path).await?;

        let stream = ReaderStream::new(self.new_data_from_file(file, uuid).await?).map(map);
        Ok(stream)
    }

    pub async fn insert<S>(&self, uuid: Uuid, mut data: S, file_ext: &str) -> Result<(), Error>
    where
        S: Stream<Item = Result<Bytes, Error>> + std::marker::Unpin,
    {
        let path = self.uuid_to_path(uuid, file_ext)?;
        let dir = path.ancestors().nth(1).unwrap();

        if tokio::fs::metadata(dir).await.is_err() {
            tokio::fs::create_dir_all(dir).await?;
        }

        let file = AtomicFile::new(path).await?;

        let mut writer = BufWriter::new(file);
        while let Some(bytes) = data.next().await {
            writer.write_all(&bytes?).await?;
        }
        writer.flush().await?;
        writer.into_inner().finalize().await?;

        Ok(())
    }

    pub async fn load_cache(&self, uuids: Vec<(Uuid, &str)>) -> Result<(), Error> {
        for (uuid, file_ext) in uuids
            .into_iter()
            .filter(|(uuid, _)| self.cache.get(uuid).is_none())
        {
            let path = self.uuid_to_path(uuid, file_ext)?;
            let file = match OpenOptions::new().read(true).open(path).await {
                Ok(f) => f,
                Err(_) => continue,
            };
            let file_len = file.metadata().await?.len();
            if file_len < self.max_cache_entry_size {
                let mut buf = Vec::with_capacity(file_len as usize);
                let mut reader = BufReader::new(file);
                reader.read_to_end(&mut buf).await?;
                self.cache.insert(uuid, Arc::new(buf));
            }
        }
        self.cache.force_update();
        Ok(())
    }
}

impl AsyncRead for Data {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let Self { source, pos, .. } = self.get_mut();
        match source {
            DataSource::Reader {
                reader,
                buf: int_buf,
                ..
            } => {
                let read = Pin::new(reader);
                let before = buf.remaining();
                let res = read.poll_read(cx, buf);
                let newbytes = before - buf.remaining();
                if newbytes != 0 {
                    int_buf.as_mut().map(|i| {
                        i.extend_from_slice(&buf.filled()[buf.filled().len() - newbytes..])
                    });
                    *pos += newbytes;
                }
                res
            }
            DataSource::Cache(cache) => {
                let rem = &cache.as_slice()[*pos..];
                let amt = std::cmp::min(buf.remaining(), cache.len() - *pos);
                let put = &rem[..amt];
                buf.put_slice(put);
                *pos += amt;
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl Drop for Data {
    fn drop(&mut self) {
        if let DataSource::Reader { buf, cache, .. } = &mut self.source {
            if let (Some(buf), Some(cache)) = (buf, cache) {
                let buf = std::mem::take(buf);
                cache.insert(self.uuid, Arc::new(buf));
            }
        }
    }
}

impl From<Arc<Vec<u8>>> for Data {
    fn from(cached: Arc<Vec<u8>>) -> Self {
        Self {
            source: DataSource::Cache(cached),
            uuid: Default::default(),
            pos: 0,
        }
    }
}

impl FsManager {
    async fn new_data_from_file(&self, file: File, uuid: Uuid) -> Result<Data, Error> {
        let (buf, cache) = if self.max_cache_entry_size > file.metadata().await?.len() {
            (Some(Vec::new()), Some(self.cache.clone()))
        } else {
            (None, None)
        };
        let reader = BufReader::new(file);
        Ok(Data {
            source: DataSource::Reader { reader, buf, cache },
            uuid,
            pos: 0,
        })
    }
}
