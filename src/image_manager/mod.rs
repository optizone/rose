use std::{path::PathBuf, sync::Arc, time::Duration};

use thiserror::Error;
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
};
use uuid::Uuid;

mod cache;
use cache::*;

#[derive(Clone)]
pub struct ImageManager {
    path: PathBuf,
    cache: Cache,
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

    pub async fn get(&self, uuid: Uuid) -> Result<Arc<Vec<u8>>, Error> {
        if let Some(cached) = self.cache.get(uuid) {
            return Ok(cached);
        }

        let path = self.uuid_to_path(uuid);
        let file = OpenOptions::new().read(true).open(path).await?;

        let size = file
            .metadata()
            .await
            .map(|m| m.len())
            .unwrap_or(2 * 1024 * 1024);

        let mut reader = tokio::io::BufReader::new(file);
        let mut buf = Vec::with_capacity(size as usize);

        reader.read_to_end(&mut buf).await?;

        let buf = Arc::new(buf);
        let copy = Arc::clone(&buf);

        self.cache.insert(uuid, copy);

        Ok(buf)
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
