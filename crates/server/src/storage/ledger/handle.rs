use crate::storage::ledger::{
    AppendReciept,
    Error::{self, CommitterDead},
    Result,
};
use std::path::PathBuf;

use bytes::Bytes;
use tokio::sync::{
    mpsc::{self, Sender},
    oneshot,
};

use crate::storage::ledger::{action::Action, committer::Committer};

#[derive(Clone)]
pub struct Handle {
    tx: Sender<Action>,
}

impl Handle {
    pub fn spawn(vault_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let committer = Committer::new(vault_dir, rx);
        tokio::spawn(committer.run());
        Self { tx }
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

    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Action::Shutdown { reply: tx }).await.is_ok() {
            let _ = rx.await;
        }
    }
}
