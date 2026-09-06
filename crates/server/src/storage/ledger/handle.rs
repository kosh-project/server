use crate::storage::{
    Payload,
    ledger::{
        AppendReciept,
        Error::{self, CommitterDead},
        Result,
    },
};
use std::{
    cmp,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use bytes::Bytes;
use tokio::{
    fs::File,
    io::AsyncSeekExt,
    sync::{
        mpsc::{self, Sender},
        oneshot,
    },
};

use crate::storage::ledger::{action::Action, committer::Committer};

#[derive(Clone)]
pub struct Handle {
    tx: Sender<Action>,
}

impl Handle {
    #[must_use]
    pub fn spawn(vault_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let committer = Committer::new(vault_dir, rx);
        tokio::spawn(committer.run());
        Self { tx }
    }

    #[must_use]
    pub const fn sender(&self) -> &Sender<Action> {
        &self.tx
    }

    pub async fn append(
        &self,
        user_id: i64,
        payload: Bytes,
    ) -> Result<AppendReciept> {
        let (reply, recv) = oneshot::channel();

        let action = Action::Append {
            user_id,
            payload,
            reply,
        };
        self.tx.send(action).await.map_err(|_| CommitterDead)?;

        recv.await.map_err(|_| CommitterDead)?
    }
}

impl Handle {
    pub async fn read_segment<P>(
        &self,
        vault_path: P,
        user_id: i64,
        file_name: &str,
        offset: u64,
    ) -> Result<File>
    where
        P: AsRef<Path>,
    {
        if file_name.contains('\\')
            || file_name.contains('/')
            || !file_name.starts_with("delta_")
        {
            return Err(Error::InvalidFileName);
        }

        let path = vault_path
            .as_ref()
            .join("ledgers")
            .join(user_id.to_string())
            .join(file_name);

        let mut file = File::open(&path).await.map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::SegmentNotFound,
            _ => Error::IoError(e),
        })?;

        let safe_offset = cmp::max(offset, 500);

        let metadata = file.metadata().await.map_err(Error::IoError)?;

        if safe_offset > metadata.len() {
            return Err(Error::InvalidOffset);
        }

        file.seek(std::io::SeekFrom::Start(safe_offset))
            .await
            .map_err(Error::IoError)?;

        Ok(file)
    }

    pub async fn shutdown(sender: &Sender<Action>) {
        let (tx, rx) = oneshot::channel();
        if sender.send(Action::Shutdown { reply: tx }).await.is_ok() {
            let _ = rx.await;
        }
    }

    pub async fn prune(&self, user_id: i64, before: u32) -> Result<()> {
        let (reply, recv) = oneshot::channel();

        self.tx
            .send(Action::Prune {
                user_id,
                before,
                reply,
            })
            .await
            .map_err(|_| Error::CommitterDead)?;

        recv.await.map_err(|_| Error::CommitterDead)?
    }
}
