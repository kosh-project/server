use std::{
    collections::{HashMap, hash_map::Entry},
    path::{Path, PathBuf},
};

use bytes::Bytes;
use chrono::Utc;
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    sync::{mpsc::Receiver, oneshot::Sender},
};

use crate::storage::ledger::{
    AppendReciept, Result,
    action::{
        Action::{self, Append, Prune, Shutdown},
        TimeStamp,
    },
    segment::Segment,
};

pub struct Committer {
    vault_path: PathBuf,
    receiver: Receiver<Action>,
    active_users: HashMap<i64, Segment>,
}

impl Committer {
    pub fn new<P>(vault_path: P, rx: Receiver<Action>) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            vault_path: vault_path.as_ref().to_owned(),
            active_users: HashMap::with_capacity(100),
            receiver: rx,
        }
    }

    pub(crate) async fn run(mut self) {
        while let Some(action) = self.receiver.recv().await {
            match action {
                Append {
                    user_id,
                    payload,
                    reply,
                } => {
                    let result = self.append(user_id, payload).await;
                    let _ = reply.send(result);
                }
                Prune {
                    user_id,
                    before,
                    reply,
                } => {
                    let _ = reply.send(Ok(()));
                    todo!("Unhandled prune from {user_id}");
                }
                Shutdown { reply } => {
                    self.shutdown(reply).await;
                    break;
                }
            }
        }
    }

    async fn prune(&mut self, user_id: i64, before: u32) -> Result<()> {
        let dir = self.vault_path.join("ledgers").join(user_id.to_string());

        let mut entries = match fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        let active_segment = self
            .active_users
            .get(&user_id)
            .map(|s| s.file_name.as_str());

        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };

            if file_name.starts_with("delta_") {
                let id_str = file_name.trim_start_matches("delta_");
                if let Ok(id) = id_str.parse::<u32>()
                    && id < before
                    && Some(file_name) != active_segment
                {
                    fs::remove_file(entry.path()).await?;
                }
            }
        }

        Ok(())
    }

    async fn append(
        &mut self,
        user_id: i64,
        payload: Bytes,
    ) -> Result<AppendReciept> {
        // todo!("Actually append the file bro");
        let active = match self.active_users.entry(user_id) {
            Entry::Occupied(segment) => segment.into_mut(),
            Entry::Vacant(entry) => {
                let segment =
                    Segment::load_or_create(&self.vault_path, user_id).await?;
                entry.insert(segment)
            }
        };

        let offset = active.current_size;

        active.file.write_all(&payload).await?;

        active.current_size += payload.len() as u64;

        Ok(AppendReciept {
            file_name: active.file_name.clone(),
            offset,
        })
    }

    async fn shutdown(mut self, reply: Sender<()>) {
        for segment in self.active_users.values_mut() {
            let _ = segment.file.sync_all().await;
        }

        let _ = reply.send(());
    }
}

async fn latest_id<P>(path: P) -> Option<u32>
where
    P: AsRef<P>,
{
    let mut max_id = 0;

    None
}
