use std::{collections::HashMap, sync::Arc, time::Duration};

use left_right::{Absorb, ReadHandle};
use tokio::{sync::mpsc::Sender, time::Instant};
use uuid::Uuid;

#[derive(Debug)]
pub enum CacheOp {
    Insert(Uuid, Arc<Vec<u8>>),
    Refresh(Uuid),
    Remove(Uuid),
    Periodic,
    ForceUpdate,
}

const TTL_SECS: u64 = 3;

impl Absorb<CacheOp> for HashMap<Uuid, (Instant, Arc<Vec<u8>>)> {
    fn absorb_first(&mut self, operation: &mut CacheOp, _: &Self) {
        match operation {
            CacheOp::Insert(uuid, data) => {
                let now = Instant::now();
                self.insert(*uuid, (now, Arc::clone(&data)));
            }
            CacheOp::Refresh(uuid) => {
                let now = Instant::now();
                self.get_mut(uuid).map(|(i, _)| *i = now);
            }
            CacheOp::Remove(uuid) => {
                self.remove(uuid);
            }
            CacheOp::Periodic => {
                self.drain_filter(|_, (i, _)| i.elapsed().as_secs() > TTL_SECS);
            }
            CacheOp::ForceUpdate => {}
        }
    }

    fn absorb_second(&mut self, operation: CacheOp, _: &Self) {
        match operation {
            CacheOp::Insert(uuid, data) => {
                let now = Instant::now();
                self.insert(uuid, (now, data));
            }
            CacheOp::Refresh(uuid) => {
                let now = Instant::now();
                self.get_mut(&uuid).map(|(i, _)| *i = now);
            }
            CacheOp::Remove(uuid) => {
                self.remove(&uuid);
            }
            CacheOp::Periodic => {
                self.drain_filter(|_, (i, _)| i.elapsed().as_secs() > TTL_SECS);
            }
            CacheOp::ForceUpdate => {}
        }
    }

    fn drop_first(self: Box<Self>) {}

    fn sync_with(&mut self, first: &Self) {
        first.iter().for_each(|(&k, (i, data))| {
            self.insert(k, (i.clone(), Arc::clone(data)));
        });
        self.drain_filter(|k, (i, _)| !first.contains_key(k) || i.elapsed().as_secs() > TTL_SECS);
    }
}

#[derive(Clone)]
pub struct Cache {
    tx: Sender<CacheOp>,
    read_handle: ReadHandle<HashMap<Uuid, (Instant, Arc<Vec<u8>>)>>,
}

impl Cache {
    pub fn new(update_every: Duration) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        let (mut writer, reader) = left_right::new();

        actix_rt::spawn(async move {
            let mut last_update = Instant::now();
            while let Some(op) = rx.recv().await {
                match op {
                    CacheOp::ForceUpdate => {
                        writer.publish();
                        last_update = Instant::now();
                    }
                    _ => {
                        writer.append(op);
                        if last_update.elapsed() > update_every {
                            writer.append(CacheOp::Periodic);
                            writer.publish();
                            last_update = Instant::now();
                        }
                    }
                }
            }
        });

        Self {
            tx,
            read_handle: reader,
        }
    }

    pub fn get(&self, uuid: &Uuid) -> Option<Arc<Vec<u8>>> {
        self.read_handle
            .enter()
            .expect("Write guard must not be dropped at this point!")
            .get(uuid)
            .map(|(_, d)| {
                let writer = self.tx.clone();
                let uuid = *uuid;
                actix_rt::spawn(async move { writer.send(CacheOp::Refresh(uuid)).await });
                Arc::clone(d)
            })
    }

    pub fn insert(&self, uuid: Uuid, data: Arc<Vec<u8>>) {
        let writer = self.tx.clone();
        actix_rt::spawn(async move {
            writer
                .send(CacheOp::Insert(uuid, data))
                .await
                .expect("Channel can't be dropped!");
        });
    }

    pub fn force_update(&self) {
        let writer = self.tx.clone();
        actix_rt::spawn(async move {
            writer
                .send(CacheOp::ForceUpdate)
                .await
                .expect("Channel can't be dropped!");
        });
    }
}
