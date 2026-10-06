#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]

use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{
    Asset,
    Tag::{GalleryItem, GalleryMeta},
};

async fn setup_db() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

async fn insert_asset(
    pool: &SqlitePool,
    user_id: i64,
    hash: &[u8],
) -> Result<()> {
    let dummy_id = format!("id_hash_{user_id}");

    sqlx::query!(
        r#"
        INSERT OR IGNORE INTO users (id, identity_hash, auth_verifier) VALUES (?, ?, ?)
        "#,
        user_id,
        dummy_id,
        "dummy_verifier"
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        r#"
        INSERT INTO assets (id, user_id, hash, size_bytes, last_modified, tag) VALUES (?, ?, ?, ?, ?, ?)
        "#,
        Uuid::new_v4().as_bytes().to_vec(),
        user_id,
        hash,
        100,
        0,
        0
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn count_owners(pool: &SqlitePool, hash: &[u8]) -> Result<i64> {
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM assets WHERE hash = ?",
        &hash[..]
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}

#[tokio::test]
async fn delete_asset_with_single_owner() -> Result<()> {
    let pool = setup_db().await?;
    let hash = b"hello_fellas_i_m_deleting_a_file";

    insert_asset(&pool, 10, hash).await?;
    Asset::delete(&pool, 10, hash).await?;
    let count = count_owners(&pool, hash).await?;

    assert_eq!(count, 0);
    Ok(())
}

#[tokio::test]
async fn attempt_to_delete_unowned_asset() -> Result<()> {
    let pool = setup_db().await?;
    let hash = b"a_dude_uploads_a_file_with_cache";

    insert_asset(&pool, 10, hash).await?;
    Asset::delete(&pool, 12, hash).await?;
    let count = count_owners(&pool, hash).await?;

    assert_eq!(count, 1);
    Ok(())
}

#[tokio::test]
async fn list_assets_filters_by_tag_and_orders_newest_first() -> Result<()> {
    let pool = setup_db().await?;
    let user_id = 99;

    sqlx::query!(
        "INSERT INTO users (id, identity_hash, auth_verifier) VALUES (?, ?, ?)",
        user_id,
        "dummy_hash_99",
        "dummy_verifier"
    )
    .execute(&pool)
    .await?;

    let insert = async move |pool: &SqlitePool,
                             hash: &[u8],
                             last_modified: i64,
                             tag: &str| {
        sqlx::query!(
            r#"
            INSERT INTO assets (id, user_id, hash, size_bytes, last_modified, tag)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            Uuid::new_v4().as_bytes().to_vec(),
            user_id,
            hash,
            1024,
            last_modified,
            tag
        )
        .execute(&pool.clone())
        .await
    };

    insert(&pool, b"hash_a", 100, "0").await?;
    insert(&pool, b"hash_b", 200, "1").await?;
    insert(&pool, b"hash_c", 300, "0").await?;

    let meta_page = Asset::list(&pool, user_id, GalleryMeta, 50, None).await?;
    assert_eq!(meta_page.assets.len(), 2);
    assert!(meta_page.next_cursor.is_none());
    assert_eq!(meta_page.assets[0].tag, GalleryMeta);
    assert_eq!(meta_page.assets[1].tag, GalleryMeta);
    assert_eq!(meta_page.assets[0].last_modified, 300);
    assert_eq!(meta_page.assets[1].last_modified, 100);

    let item_page = Asset::list(&pool, user_id, GalleryItem, 50, None).await?;
    assert_eq!(item_page.assets.len(), 1);
    assert_eq!(item_page.assets[0].tag, GalleryItem);
    assert_eq!(item_page.assets[0].last_modified, 200);

    Ok(())
}

#[tokio::test]
async fn list_assets_pagination_with_cursor_and_tie_breaking() -> Result<()> {
    let pool = setup_db().await?;
    let user_id = 42;

    sqlx::query!(
        "INSERT INTO users (id, identity_hash, auth_verifier) VALUES (?, ?, ?)",
        user_id,
        "dummy_hash_42",
        "dummy_verifier"
    )
    .execute(&pool)
    .await?;

    // Insert 3 assets with identical timestamp (1000)
    for i in 1u8..=3u8 {
        let id = vec![i; 16];
        let hash = format!("hash_{i}").into_bytes();
        sqlx::query!(
            r#"
            INSERT INTO assets (id, user_id, hash, size_bytes, last_modified, tag)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            id,
            user_id,
            hash,
            512,
            1000,
            "0"
        )
        .execute(&pool)
        .await?;
    }

    // Page 1: limit 2
    let page1 = Asset::list(&pool, user_id, GalleryMeta, 2, None).await?;
    assert_eq!(page1.assets.len(), 2);
    assert!(page1.next_cursor.is_some());

    // Page 2: pass cursor from page 1
    let page2 =
        Asset::list(&pool, user_id, GalleryMeta, 2, page1.next_cursor.as_ref())
            .await?;
    assert_eq!(page2.assets.len(), 1);
    assert!(page2.next_cursor.is_none());

    // Verify tie-breaking: disjoint rows across pages
    assert_ne!(page1.assets[0].hash, page2.assets[0].hash);
    assert_ne!(page1.assets[1].hash, page2.assets[0].hash);

    Ok(())
}
