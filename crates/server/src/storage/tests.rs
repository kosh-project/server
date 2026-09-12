// use std::path::PathBuf;

use std::path::PathBuf;

use anyhow::Result;
use tmpdir::TmpDir;

use crate::storage::transaction::Transaction;

use super::*;

pub(super) async fn with_temp_service<F, T, Fut>(func: F) -> Result<T>
where
    F: FnOnce(Service) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let temp_dir = TmpDir::new("vault").await?;

    let storage_service = Service::new(temp_dir.to_path_buf());

    func(storage_service).await
}

pub(super) async fn with_temp_transaction<F, T, Fut>(
    func: F,
) -> anyhow::Result<T>
where
    F: Fn(Transaction, PathBuf) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let temp_dir = TmpDir::new("vault").await?;

    let transaction = Transaction::new(temp_dir.to_path_buf());

    func(transaction, temp_dir.to_path_buf()).await
}
