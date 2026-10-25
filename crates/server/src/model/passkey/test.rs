#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_wraps)]

use super::*;
use crate::model::user::User;
use anyhow::{Context, Result};
use sqlx::SqlitePool;

async fn setup_db() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    sqlx::migrate!().run(&pool).await.context("migrate")?;
    Ok(pool)
}

async fn create_test_user(pool: &SqlitePool, id_byte: u8) -> Result<i64> {
    let identity_hash = [id_byte; 32];
    let verifier = format!("verifier_{id_byte}");
    let wrapped_vault_key = [id_byte; 60];
    User::create(pool, &identity_hash, verifier, &wrapped_vault_key).await?;
    let user = User::verify(pool, &identity_hash, format!("verifier_{id_byte}"))
        .await?
        .context("user must exist")?;
    Ok(user.id)
}

#[tokio::test]
async fn record_create_and_find_success() -> Result<()> {
    let pool = setup_db().await?;
    let user_id = create_test_user(&pool, 1).await?;

    let record = Record {
        credential_id: vec![1, 2, 3, 4],
        user_id,
        passkey_data: b"serialized_passkey_state".to_vec(),
        counter: 0,
        encrypted_metadata: vec![10, 20, 30],
        created_at: 1000,
        last_used: 0,
    };

    Record::create(&pool, &record).await?;

    let found = Record::find(&pool, &record.credential_id).await?;
    assert_eq!(found, Some(record));

    Ok(())
}

#[tokio::test]
async fn record_find_nonexistent_returns_none() -> Result<()> {
    let pool = setup_db().await?;
    let found = Record::find(&pool, b"nonexistent_cred").await?;
    assert!(found.is_none());

    Ok(())
}

#[tokio::test]
async fn record_create_duplicate_credential_id_rejected() -> Result<()> {
    let pool = setup_db().await?;
    let user_id = create_test_user(&pool, 2).await?;

    let record1 = Record {
        credential_id: vec![5, 6, 7, 8],
        user_id,
        passkey_data: b"data1".to_vec(),
        counter: 0,
        encrypted_metadata: vec![1],
        created_at: 1000,
        last_used: 0,
    };
    Record::create(&pool, &record1).await?;

    let record2 = Record {
        credential_id: vec![5, 6, 7, 8],
        user_id,
        passkey_data: b"data2".to_vec(),
        counter: 0,
        encrypted_metadata: vec![2],
        created_at: 2000,
        last_used: 0,
    };
    let duplicate_res = Record::create(&pool, &record2).await;
    assert!(duplicate_res.is_err());

    Ok(())
}

#[tokio::test]
async fn record_create_foreign_key_violation() -> Result<()> {
    let pool = setup_db().await?;
    let nonexistent_user_id = 9999;

    let record = Record {
        credential_id: vec![9, 10, 11],
        user_id: nonexistent_user_id,
        passkey_data: b"data".to_vec(),
        counter: 0,
        encrypted_metadata: vec![],
        created_at: 1000,
        last_used: 0,
    };

    let res = Record::create(&pool, &record).await;
    assert!(res.is_err());

    Ok(())
}

#[tokio::test]
async fn record_create_metadata_length_check_constraint() -> Result<()> {
    let pool = setup_db().await?;
    let user_id = create_test_user(&pool, 3).await?;

    // Max allowed metadata is 1024 bytes
    let valid_record = Record {
        credential_id: vec![12, 13, 14],
        user_id,
        passkey_data: b"data".to_vec(),
        counter: 0,
        encrypted_metadata: vec![0u8; 1024],
        created_at: 1000,
        last_used: 0,
    };
    assert!(Record::create(&pool, &valid_record).await.is_ok());

    // 1025 bytes exceeds check constraint
    let invalid_record = Record {
        credential_id: vec![15, 16, 17],
        user_id,
        passkey_data: b"data".to_vec(),
        counter: 0,
        encrypted_metadata: vec![0u8; 1025],
        created_at: 1000,
        last_used: 0,
    };
    assert!(Record::create(&pool, &invalid_record).await.is_err());

    Ok(())
}

#[tokio::test]
async fn record_list_for_user_ordering_and_summaries() -> Result<()> {
    let pool = setup_db().await?;
    let user1 = create_test_user(&pool, 4).await?;
    let user2 = create_test_user(&pool, 5).await?;

    let r1 = Record {
        credential_id: vec![0xaa, 0xbb],
        user_id: user1,
        passkey_data: b"data1".to_vec(),
        counter: 1,
        encrypted_metadata: vec![0x01, 0x02],
        created_at: 100,
        last_used: 10,
    };
    let r2 = Record {
        credential_id: vec![0xcc, 0xdd],
        user_id: user1,
        passkey_data: b"data2".to_vec(),
        counter: 2,
        encrypted_metadata: vec![0x03, 0x04],
        created_at: 200,
        last_used: 20,
    };
    let r3 = Record {
        credential_id: vec![0xee, 0xff],
        user_id: user2,
        passkey_data: b"data3".to_vec(),
        counter: 0,
        encrypted_metadata: vec![0x05],
        created_at: 150,
        last_used: 0,
    };

    Record::create(&pool, &r1).await?;
    Record::create(&pool, &r2).await?;
    Record::create(&pool, &r3).await?;

    let list = Record::list_for_user(&pool, user1).await?;
    assert_eq!(list.len(), 2);
    // Ordered created_at DESC -> r2 first, then r1
    assert_eq!(list[0].credential_id, "ccdd");
    assert_eq!(list[0].encrypted_metadata, "0304");
    assert_eq!(list[0].created_at, 200);
    assert_eq!(list[0].last_used, 20);

    assert_eq!(list[1].credential_id, "aabb");
    assert_eq!(list[1].encrypted_metadata, "0102");
    assert_eq!(list[1].created_at, 100);
    assert_eq!(list[1].last_used, 10);

    // User with no credentials returns empty
    let empty_user = create_test_user(&pool, 6).await?;
    let empty_list = Record::list_for_user(&pool, empty_user).await?;
    assert!(empty_list.is_empty());

    Ok(())
}

#[tokio::test]
async fn record_update_counter_updates_passkey_data_and_timestamps() -> Result<()> {
    let pool = setup_db().await?;
    let user_id = create_test_user(&pool, 7).await?;

    let record = Record {
        credential_id: vec![20, 21, 22],
        user_id,
        passkey_data: b"initial_state".to_vec(),
        counter: 0,
        encrypted_metadata: vec![1, 2],
        created_at: 500,
        last_used: 0,
    };
    Record::create(&pool, &record).await?;

    Record::update_counter(
        &pool,
        &record.credential_id,
        b"updated_state",
        77,
        123_456,
    )
    .await?;

    let found = Record::find(&pool, &record.credential_id)
        .await?
        .context("record must exist")?;
    assert_eq!(found.counter, 77);
    assert_eq!(found.last_used, 123_456);
    assert_eq!(found.passkey_data, b"updated_state");
    assert_eq!(found.created_at, 500);

    Ok(())
}

#[tokio::test]
async fn record_update_counter_nonexistent_does_not_error() -> Result<()> {
    let pool = setup_db().await?;
    let res = Record::update_counter(
        &pool,
        b"nonexistent_cred",
        b"data",
        1,
        1000,
    )
    .await;
    assert!(res.is_ok());

    let found = Record::find(&pool, b"nonexistent_cred").await?;
    assert!(found.is_none());

    Ok(())
}

#[tokio::test]
async fn record_delete_by_owner_and_unowned() -> Result<()> {
    let pool = setup_db().await?;
    let user1 = create_test_user(&pool, 8).await?;
    let user2 = create_test_user(&pool, 9).await?;

    let record = Record {
        credential_id: vec![30, 31, 32],
        user_id: user1,
        passkey_data: b"data".to_vec(),
        counter: 0,
        encrypted_metadata: vec![],
        created_at: 1000,
        last_used: 0,
    };
    Record::create(&pool, &record).await?;

    // Attempting to delete using wrong user_id returns false
    let deleted_by_other =
        Record::delete(&pool, &record.credential_id, user2).await?;
    assert!(!deleted_by_other);
    assert!(Record::find(&pool, &record.credential_id).await?.is_some());

    // Deleting with correct user_id returns true
    let deleted_by_owner =
        Record::delete(&pool, &record.credential_id, user1).await?;
    assert!(deleted_by_owner);
    assert!(Record::find(&pool, &record.credential_id).await?.is_none());

    // Deleting again returns false
    let deleted_again =
        Record::delete(&pool, &record.credential_id, user1).await?;
    assert!(!deleted_again);

    Ok(())
}

#[tokio::test]
async fn record_exclude_list_for_user() -> Result<()> {
    let pool = setup_db().await?;
    let user1 = create_test_user(&pool, 10).await?;
    let user2 = create_test_user(&pool, 11).await?;

    // User with 0 passkeys returns None
    let empty_excluded = Record::exclude_list_for_user(&pool, user1).await?;
    assert!(empty_excluded.is_none());

    // Insert 2 passkeys for user 1
    let r1 = Record {
        credential_id: vec![40, 41],
        user_id: user1,
        passkey_data: b"d1".to_vec(),
        counter: 0,
        encrypted_metadata: vec![],
        created_at: 100,
        last_used: 0,
    };
    let r2 = Record {
        credential_id: vec![42, 43],
        user_id: user1,
        passkey_data: b"d2".to_vec(),
        counter: 0,
        encrypted_metadata: vec![],
        created_at: 200,
        last_used: 0,
    };
    Record::create(&pool, &r1).await?;
    Record::create(&pool, &r2).await?;

    let excluded = Record::exclude_list_for_user(&pool, user1)
        .await?
        .context("excluded credentials must exist")?;
    assert_eq!(excluded.len(), 2);
    assert!(excluded.contains(&CredentialID::from(vec![40, 41])));
    assert!(excluded.contains(&CredentialID::from(vec![42, 43])));

    // User 2 still has no excluded credentials
    assert!(Record::exclude_list_for_user(&pool, user2).await?.is_none());

    Ok(())
}

#[test]
fn record_new_serialization() -> Result<()> {
    let raw_json = r#"{
        "cred": {
            "cred_id": [1, 2, 3, 4],
            "cred": {
                "type_": "ES256",
                "key": {
                    "EC_EC2": {
                        "curve": "SECP256R1",
                        "x": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                        "y": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
                    }
                }
            },
            "counter": 0,
            "transports": null,
            "user_verified": true,
            "backup_eligible": false,
            "backup_state": false,
            "registration_policy": "discouraged",
            "extensions": {},
            "attestation": {
                "data": "None",
                "metadata": "None"
            },
            "attestation_format": "None"
        }
    }"#;

    let passkey: Passkey = serde_json::from_str(raw_json)
        .context("deserializing sample Passkey")?;

    let metadata = vec![0xde, 0xad, 0xbe, 0xef];
    let record = Record::new(42, &passkey, metadata.clone())?;

    assert_eq!(record.user_id, 42);
    assert_eq!(record.credential_id, vec![1, 2, 3, 4]);
    assert_eq!(record.counter, 0);
    assert_eq!(record.last_used, 0);
    assert_eq!(record.encrypted_metadata, metadata);
    assert!(record.created_at > 0);

    // Verify passkey_data deserializes back to the same passkey
    let roundtripped: Passkey = serde_json::from_slice(&record.passkey_data)?;
    assert_eq!(roundtripped.cred_id().as_ref(), &[1, 2, 3, 4]);

    Ok(())
}
