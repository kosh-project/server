#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::as_conversions)]

use std::path::PathBuf;

use anyhow::Result;
use blake3::Hasher;
use bytes::Bytes;
use std::io::{Error as IoErr, ErrorKind};
use tmpdir::TmpDir;
use tokio::io::AsyncReadExt;

use crate::storage::{
    Error::{CreateTempFile, InvalidFileName, NotFound},
    Payload,
    service::Service,
    transaction::Transaction,
};

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

// =========================================================================
// Service Unit Tests
// =========================================================================

#[tokio::test]
async fn reject_invalid_filename() -> Result<()> {
    with_temp_service(|service| async move {
        // Reject for any occurrence of forward slash
        let result = service.begin_transaction(&"o///reo/hiuh//i");
        assert!(result.is_err());
        assert!(matches!(result, Err(InvalidFileName)));

        let result = service.begin_transaction(&"");
        assert!(result.is_err());
        assert!(matches!(result, Err(InvalidFileName)));

        let result = service.begin_transaction(&"../../../../etc/passwd");
        assert!(result.is_err());
        assert!(matches!(result, Err(InvalidFileName)));

        Ok(())
    })
    .await
}

#[tokio::test]
async fn validation_success() -> Result<()> {
    with_temp_service(|service| async move {
        service.begin_transaction(&"oreo.tmp.jks")?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn concurrent_write_collisions_dont_panic() -> Result<()> {
    with_temp_service(|service| async move {
        let service_a = service.clone();
        let service_b = service.clone();

        let task_a = tokio::spawn(async move {
            let chunks: Vec<Result<Bytes, IoErr>> =
                vec![Ok(Bytes::from("some_data"))];
            let stream = futures::stream::iter(chunks);

            service_a
                .try_save("dev1_upload.rs", Payload::new(9u64, stream))
                .await
        });

        let task_b = tokio::spawn(async move {
            let payload: Vec<Result<Bytes, IoErr>> =
                vec![Ok(Bytes::from("some_data"))];
            let stream = futures::stream::iter(payload);

            service_b
                .try_save("some_other_file.rs", Payload::new(9u64, stream))
                .await
        });

        let (result_a, result_b) = tokio::join!(task_a, task_b);

        let metadata_a = result_a?.expect("task_a failed");
        let metadata_b = result_b?.expect("task_b failed");

        assert_eq!(
            metadata_a.hash.to_string(),
            metadata_b.hash.to_string()
        );

        let expected_path =
            service.vault_path.join(metadata_a.hash.to_string());
        assert!(expected_path.exists());

        Ok(())
    })
    .await
}

#[tokio::test]
async fn service_get_blob_success() -> Result<()> {
    with_temp_service(|service| async move {
        let content = b"cas_blob_test_content";
        let chunks: Vec<Result<Bytes, IoErr>> = vec![Ok(Bytes::from_static(content))];
        let stream = futures::stream::iter(chunks);

        let metadata = service
            .try_save("sample.bin", Payload::new(content.len() as u64, stream))
            .await?;

        let hash_str = metadata.hash.to_string();
        let mut file = service.get_blob(&hash_str).await?;

        let mut read_bytes = Vec::new();
        file.read_to_end(&mut read_bytes).await?;
        assert_eq!(read_bytes, content);

        Ok(())
    })
    .await
}

#[tokio::test]
async fn service_get_blob_not_found() -> Result<()> {
    with_temp_service(|service| async move {
        let result = service.get_blob("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(NotFound)));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn service_delete_blob_success() -> Result<()> {
    with_temp_service(|service| async move {
        let content = b"delete_me_soon";
        let chunks: Vec<Result<Bytes, IoErr>> = vec![Ok(Bytes::from_static(content))];
        let stream = futures::stream::iter(chunks);

        let metadata = service
            .try_save("ephemeral.bin", Payload::new(content.len() as u64, stream))
            .await?;

        let hash_str = metadata.hash.to_string();
        let file_path = service.vault_path.join(&hash_str);
        assert!(file_path.exists());

        service.delete_blob(&hash_str).await?;
        assert!(!file_path.exists());

        Ok(())
    })
    .await
}

#[tokio::test]
async fn service_delete_blob_idempotent() -> Result<()> {
    with_temp_service(|service| async move {
        let result = service.delete_blob("nonexistent_hash_string").await;
        assert!(result.is_ok());
        Ok(())
    })
    .await
}

// =========================================================================
// Transaction Unit Tests
// =========================================================================

#[tokio::test]
async fn successful_commit_and_hash() -> anyhow::Result<()> {
    with_temp_transaction(async move |transaction, vault_path| {
        let chunks: Vec<Result<Bytes, IoErr>> = vec![
            Ok(Bytes::from("hello")),
            Ok(Bytes::from(" ")),
            Ok(Bytes::from("world")),
        ];

        let payload = Payload::new(11_u64, futures::stream::iter(chunks));

        let result = transaction.commit(payload).await;

        let metadata = result?;

        let target_path = vault_path.join(metadata.hash.to_string());

        let mut hasher = Hasher::new();
        let bytes = tokio::fs::read(target_path).await?;
        hasher.update(&bytes);

        let expected_hash = hasher.finalize().to_string();

        assert_eq!(expected_hash, metadata.hash.to_string());

        Ok(())
    })
    .await
}

#[tokio::test]
async fn zero_byte_stream_creates_empty_file() -> anyhow::Result<()> {
    with_temp_transaction(async move |transaction, vault_path| {
        let chunks: Vec<Result<Bytes, IoErr>> = Vec::new();

        let payload = Payload::new(0u64, futures::stream::iter(chunks));

        let result = transaction.commit(payload).await;

        let metadata = result?;

        assert_eq!(metadata.size, 0);

        let target_path = vault_path.join(metadata.hash.to_string());

        assert!(target_path.exists());

        let expected_hash = Hasher::new().finalize().to_string();

        assert_eq!(metadata.hash.to_string(), expected_hash);

        Ok(())
    })
    .await
}

#[tokio::test]
async fn aborted_test_cleans_up_garbage() -> anyhow::Result<()> {
    with_temp_transaction(async move |transaction, _vault_path| {
        let temp_path = transaction.temp_path().to_owned();

        let chunks: Vec<Result<Bytes, IoErr>> = vec![
            Ok(Bytes::from("good bytes")),
            Err(IoErr::new(ErrorKind::ConnectionAborted, "Wifi dies, lol")),
        ];

        let payload = Payload::new(20_u64, futures::stream::iter(chunks));

        let result = transaction.commit(payload).await;

        assert!(result.is_err());

        assert!(!temp_path.exists());

        Ok(())
    })
    .await
}

#[tokio::test]
async fn transaction_fails_if_vault_missing() -> anyhow::Result<()> {
    let vault = PathBuf::from("/tmp/path/that/possibly/doesnt/exist/lol");
    let transaction = Transaction::new(vault);

    let chunks: Vec<Result<Bytes, IoErr>> =
        vec![Ok(Bytes::from("data_data"))];
    let f_stream = futures::stream::iter(chunks);

    let payload = Payload::new(9u64, f_stream);

    let result = transaction.commit(payload).await;

    assert!(result.is_err());

    assert!(
        matches!(result, Err(CreateTempFile { .. })),
        "Expected Err(CreateTempFile)"
    );

    Ok(())
}

#[tokio::test]
async fn hardcoded_hash_correctness() -> anyhow::Result<()> {
    with_temp_transaction(async move |transaction, _vault_path| {
        let payload: Vec<Result<Bytes, IoErr>> = vec![Ok(Bytes::from("hello world"))];
        let f_stream = futures::stream::iter(payload);

        let payload = Payload::new(11_u64, f_stream);

        let metadata = transaction.commit(payload).await?;

        // Pre-calculated Blake3 hash of "hello world"
        let expected_hash = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";

        assert_eq!(metadata.hash.to_string(), expected_hash);

        Ok(())
    })
    .await
}

#[tokio::test]
async fn mismatch_content_fails_plus_cleans_up() -> anyhow::Result<()> {
    with_temp_transaction(async move |transaction, _| {
        let temp_path = transaction.temp_path().to_owned();

        let chunks: Vec<Result<Bytes, IoErr>> =
            vec![Ok(Bytes::from("Halo there"))];

        let payload = Payload::new(67u64, futures::stream::iter(chunks));

        let result = transaction.commit(payload).await;

        assert!(result.is_err());

        assert!(!temp_path.exists());

        Ok(())
    })
    .await
}
