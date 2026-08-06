use std::path::PathBuf;

use tmpdir::TmpDir;

use crate::storage::transaction::Transaction;

use super::*;

pub(super) async fn with_temp_service<F, T, Fut>(func: F) -> T
where
    F: FnOnce(Service) -> Fut,
    Fut: Future<Output = T>,
{
    let temp_dir = TmpDir::new("vault").await.unwrap();

    let storage_service = Service::new(temp_dir.to_path_buf());

    func(storage_service).await
}

pub(super) async fn with_temp_transaction<F, T, Fut>(func: F) -> T
where
    F: Fn(Transaction, PathBuf) -> Fut,
    Fut: Future<Output = T>,
{
    let temp_dir = TmpDir::new("vault").await.unwrap();

    let transaction = Transaction::new(temp_dir.to_path_buf());

    func(transaction, temp_dir.to_path_buf()).await
}
