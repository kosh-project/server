#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]

use std::collections::HashSet;

use super::*;
use crate::app::mailbox::Error::{
    AlreadyFulfilled, AlreadyJoined, NotFound, NotReady, Unauthorized,
};

#[test]
fn crockford_charset_adherence_and_length() {
    for _ in 0..10_000 {
        let id = Mailbox::generate_id();
        assert_eq!(id.len(), 8, "Mailbox ID must be exactly 8 characters");

        for b in id.bytes() {
            assert!(
                BASE32_ALPHABET.contains(&b),
                "Character '{b}' is not in Crockford Base32 alphabet"
            );
            // Crockford Base32 excludes i, l, o, u to prevent visual ambiguity
            assert_ne!(b, b'i');
            assert_ne!(b, b'l');
            assert_ne!(b, b'o');
            assert_ne!(b, b'u');
            assert!(!b.is_ascii_uppercase());
        }
    }
}

#[test]
fn crockford_alphabet_full_coverage() {
    let mut seen_chars = HashSet::new();
    for _ in 0..2_000 {
        let id = Mailbox::generate_id();
        for ch in id.chars() {
            seen_chars.insert(ch);
        }
    }

    assert_eq!(
        seen_chars.len(),
        32,
        "All 32 Crockford Base32 characters must be generated without bias"
    );
    for &expected_byte in BASE32_ALPHABET {
        assert!(
            seen_chars.contains(&char::from(expected_byte)),
            "Alphabet character '{}' was never generated",
            char::from(expected_byte)
        );
    }
}

#[tokio::test]
async fn test_flow_a_lifecycle() {
    let mailbox = Mailbox::new();
    let caller_id = 42;
    let pub_temp = vec![0xaa; 32];

    // 1. Unauthenticated receiver opens mailbox
    let receipt = mailbox.open(pub_temp).await;
    assert_eq!(receipt.id.len(), 8);
    assert_eq!(receipt.token.hex.len(), 64);

    // 2. Initial pickup before fulfillment returns Ok(None)
    let poll = mailbox.pickup(&receipt.id, &receipt.token.hex).await;
    assert_eq!(poll.unwrap(), None);

    // 3. Pickup with invalid token returns Unauthorized
    let bad_token = "00".repeat(32);
    let bad_poll = mailbox.pickup(&receipt.id, &bad_token).await;
    assert!(matches!(bad_poll, Err(Unauthorized)));

    // 4. Authenticated sender fulfills with ciphertext
    let ciphertext = vec![0xca, 0xfe, 0xba, 0xbe];
    let fulfill_res = mailbox
        .fulfill(&receipt.id, caller_id, ciphertext.clone())
        .await;
    assert!(fulfill_res.is_ok());

    // 5. Double fulfill returns AlreadyFulfilled
    let double_fulfill =
        mailbox.fulfill(&receipt.id, caller_id, vec![1, 2, 3]).await;
    assert!(matches!(double_fulfill, Err(AlreadyFulfilled)));

    // 6. Receiver picks up payload and burns cache entry
    let pickup_res = mailbox.pickup(&receipt.id, &receipt.token.hex).await;
    assert_eq!(pickup_res.unwrap(), Some(ciphertext));

    // 7. Second pickup returns NotFound
    let second_pickup = mailbox.pickup(&receipt.id, &receipt.token.hex).await;
    assert!(matches!(second_pickup, Err(NotFound)));

    // 8. Fulfill after burn returns NotFound
    let post_fulfill =
        mailbox.fulfill(&receipt.id, caller_id, vec![4, 5, 6]).await;
    assert!(matches!(post_fulfill, Err(NotFound)));
}

#[tokio::test]
async fn test_flow_b_lifecycle() {
    let mailbox = Mailbox::new();
    let owner_id = 100;
    let attacker_id = 999;
    let pub_temp = vec![0xbb; 32];
    let ciphertext = vec![1, 2, 3, 4, 5, 6, 7, 8];

    // 1. Authenticated owner offers mailbox
    let id = mailbox.offer(owner_id).await;
    assert_eq!(id.len(), 8);

    // 2. Owner checks status while waiting: Ok(None)
    let initial_status = mailbox.status(&id, owner_id).await;
    assert_eq!(initial_status.unwrap(), None);

    // 3. Cross-tenant status check returns NotFound
    let cross_status = mailbox.status(&id, attacker_id).await;
    assert!(matches!(cross_status, Err(NotFound)));

    // 4. Early fulfill before join returns NotReady
    let early_fulfill =
        mailbox.fulfill(&id, owner_id, ciphertext.clone()).await;
    assert!(matches!(early_fulfill, Err(NotReady)));

    // 5. Unauthenticated device joins
    let join_res = mailbox.join(&id, pub_temp.clone()).await;
    assert!(join_res.is_ok());
    let token_hex = join_res.unwrap();
    assert_eq!(token_hex.len(), 64);

    // 6. Double join returns AlreadyJoined
    let second_join = mailbox.join(&id, pub_temp.clone()).await;
    assert!(matches!(second_join, Err(AlreadyJoined)));

    // 7. Owner polls status after join: Ok(Some(pub_temp))
    let ready_status = mailbox.status(&id, owner_id).await;
    assert_eq!(ready_status.unwrap(), Some(pub_temp));

    // 8. Cross-tenant status poll returns NotFound
    let cross_status_2 = mailbox.status(&id, attacker_id).await;
    assert!(matches!(cross_status_2, Err(NotFound)));

    // 9. Cross-tenant fulfill returns NotFound (anti-enumeration)
    let cross_fulfill =
        mailbox.fulfill(&id, attacker_id, ciphertext.clone()).await;
    assert!(matches!(cross_fulfill, Err(NotFound)));

    // 10. Owner fulfills successfully
    let fulfill_res = mailbox.fulfill(&id, owner_id, ciphertext.clone()).await;
    assert!(fulfill_res.is_ok());

    // 11. Double fulfill returns AlreadyFulfilled
    let double_fulfill =
        mailbox.fulfill(&id, owner_id, ciphertext.clone()).await;
    assert!(matches!(double_fulfill, Err(AlreadyFulfilled)));

    // 12. Pickup with wrong token returns Unauthorized
    let bad_pickup = mailbox.pickup(&id, "wrong_token").await;
    assert!(matches!(bad_pickup, Err(Unauthorized)));

    // 13. Pickup with valid token returns ciphertext and burns entry
    let pickup_res = mailbox.pickup(&id, &token_hex).await;
    assert_eq!(pickup_res.unwrap(), Some(ciphertext));

    // 14. Subsequent pickup returns NotFound
    let retry_pickup = mailbox.pickup(&id, &token_hex).await;
    assert!(matches!(retry_pickup, Err(NotFound)));
}

#[tokio::test]
async fn test_nonexistent_mailbox_returns_not_found() {
    let mailbox = Mailbox::new();
    let fake_id = "00000000";
    let user_id = 1;

    assert!(matches!(
        mailbox.join(fake_id, vec![0; 32]).await,
        Err(NotFound)
    ));
    assert!(matches!(
        mailbox.status(fake_id, user_id).await,
        Err(NotFound)
    ));
    assert!(matches!(
        mailbox.fulfill(fake_id, user_id, vec![1, 2, 3]).await,
        Err(NotFound)
    ));
    assert!(matches!(
        mailbox.pickup(fake_id, "dummy_token").await,
        Err(NotFound)
    ));
}

#[tokio::test]
async fn test_concurrent_pickup_single_winner() {
    let mailbox = Mailbox::new();
    let receipt = mailbox.open(vec![0x11; 32]).await;
    let payload = vec![1, 2, 3, 4, 5, 6, 7, 8];
    mailbox
        .fulfill(&receipt.id, 1, payload.clone())
        .await
        .unwrap();

    let mut tasks = Vec::new();
    for _ in 0..20 {
        let mb = mailbox.clone();
        let id_clone = receipt.id.clone();
        let token_clone = receipt.token.hex.clone();
        tasks.push(tokio::spawn(async move {
            mb.pickup(&id_clone, &token_clone).await
        }));
    }

    let mut success_count = 0;
    let mut not_found_count = 0;

    for task in tasks {
        let res = task.await.unwrap();
        assert!(
            matches!(res, Ok(Some(_)) | Err(NotFound)),
            "Outcome must be either Ok(Some) or Err(NotFound)"
        );
        if let Ok(Some(p)) = res {
            assert_eq!(p, payload);
            success_count += 1;
        } else if matches!(res, Err(NotFound)) {
            not_found_count += 1;
        }
    }

    assert_eq!(
        success_count, 1,
        "Exactly one concurrent pickup must succeed"
    );
    assert_eq!(
        not_found_count, 19,
        "All other concurrent pickups must receive NotFound"
    );
}

#[tokio::test]
async fn test_concurrent_fulfill_single_winner() {
    let mailbox = Mailbox::new();
    let receipt = mailbox.open(vec![0x22; 32]).await;

    let mut tasks = Vec::new();
    for i in 0u8..20u8 {
        let mb = mailbox.clone();
        let id_clone = receipt.id.clone();
        let payload = vec![i; 10];
        tasks.push(tokio::spawn(async move {
            mb.fulfill(&id_clone, 1, payload).await
        }));
    }

    let mut success_count = 0;
    let mut conflict_count = 0;

    for task in tasks {
        let res = task.await.unwrap();
        assert!(
            res.is_ok() || matches!(res, Err(AlreadyFulfilled)),
            "Outcome must be either Ok(()) or Err(AlreadyFulfilled)"
        );
        if res.is_ok() {
            success_count += 1;
        } else if matches!(res, Err(AlreadyFulfilled)) {
            conflict_count += 1;
        }
    }

    assert_eq!(
        success_count, 1,
        "Exactly one concurrent fulfill must succeed"
    );
    assert_eq!(
        conflict_count, 19,
        "All other concurrent fulfills must receive AlreadyFulfilled"
    );

    let picked = mailbox
        .pickup(&receipt.id, &receipt.token.hex)
        .await
        .unwrap();
    assert!(picked.is_some());
}

#[tokio::test]
async fn test_cache_capacity_eviction_behavior() {
    let mailbox = Mailbox::new();

    let mut ids = Vec::with_capacity(1_050);
    for _ in 0..1_050 {
        ids.push(mailbox.offer(1).await);
    }

    assert_eq!(ids.len(), 1_050);

    mailbox.cache.run_pending_tasks().await;
    assert!(mailbox.cache.entry_count() <= 1_000);
}

#[tokio::test]
async fn test_concurrent_mailbox_id_uniqueness() {
    let mailbox = Mailbox::new();

    let mut tasks = Vec::new();
    for i in 0..50 {
        let mb = mailbox.clone();
        tasks.push(tokio::spawn(async move {
            if i % 2 == 0 {
                mb.open(vec![i; 32]).await.id
            } else {
                mb.offer(i64::from(i)).await
            }
        }));
    }

    let mut ids = HashSet::new();
    for task in tasks {
        let id = task.await.unwrap();
        assert_eq!(id.len(), 8);
        assert!(
            ids.insert(id),
            "Duplicate mailbox ID generated concurrently"
        );
    }
    assert_eq!(ids.len(), 50);
}
