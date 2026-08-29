use bytes::Bytes;
use tokio::sync::oneshot::Sender;

use crate::storage::ledger::Result;

#[derive(Debug)]
pub enum Action {
    Append {
        user_id: i64,
        payload: Bytes,
        reply: Sender<Result<AppendReciept>>,
    },
    Prune {
        user_id: i64,
        before: u32,
        reply: Sender<Result<()>>,
    },
    Shutdown {
        reply: Sender<()>,
    },
}

pub type TimeStamp = i64;

#[derive(Debug, Clone)]
pub struct AppendReciept {
    pub file_name: String,
    pub offset: u64,
}
