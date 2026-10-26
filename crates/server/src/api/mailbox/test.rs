#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]

use super::*;
use crate::api::Error::BadRequest;
use serde_json::json;

#[test]
fn fulfill_payload_length_boundaries() {
    // 1. Under lower boundary: 0 hex chars
    let req = fulfill::Request {
        payload: String::new(),
    };
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest(
            "Payload must be between 120 and 8192 hex chars (or 60 to 4096 bytes)"
        ))
    ));

    // 2. Under lower boundary: 118 hex chars (59 bytes)
    let req = fulfill::Request {
        payload: "00".repeat(59),
    };
    assert_eq!(req.payload.len(), 118);
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest(
            "Payload must be between 120 and 8192 hex chars (or 60 to 4096 bytes)"
        ))
    ));

    // 3. Exact lower boundary: 120 hex chars (60 bytes)
    let req = fulfill::Request {
        payload: "ab".repeat(60),
    };
    assert_eq!(req.payload.len(), 120);
    let res = req.parse_payload();
    assert!(res.is_ok());
    let bytes = res.unwrap();
    assert_eq!(bytes.len(), 60);
    assert_eq!(bytes, vec![0xab; 60]);

    // 4. Exact upper boundary: 8192 hex chars (4096 bytes)
    let req = fulfill::Request {
        payload: "cd".repeat(4096),
    };
    assert_eq!(req.payload.len(), 8192);
    let res = req.parse_payload();
    assert!(res.is_ok());
    let bytes = res.unwrap();
    assert_eq!(bytes.len(), 4096);
    assert_eq!(bytes, vec![0xcd; 4096]);

    // 5. Over upper boundary: 8194 hex chars (4097 bytes)
    let req = fulfill::Request {
        payload: "cd".repeat(4097),
    };
    assert_eq!(req.payload.len(), 8194);
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest(
            "Payload must be between 120 and 8192 hex chars (or 60 to 4096 bytes)"
        ))
    ));
}

#[test]
fn fulfill_payload_invalid_hex() {
    // 1. Odd length within range: 121 hex characters
    let mut odd_payload = "00".repeat(60);
    odd_payload.push('a');
    assert_eq!(odd_payload.len(), 121);
    let req = fulfill::Request {
        payload: odd_payload,
    };
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest("Invalid hexadecimal payload"))
    ));

    // 2. Non-hex characters within valid length range (120 chars)
    let mut bad_chars_payload = "ab".repeat(59);
    bad_chars_payload.push_str("zz");
    assert_eq!(bad_chars_payload.len(), 120);
    let req = fulfill::Request {
        payload: bad_chars_payload,
    };
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest("Invalid hexadecimal payload"))
    ));

    // 3. Payload with punctuation/symbols (120 chars)
    let mut sym_payload = "12".repeat(59);
    sym_payload.push_str("!!");
    assert_eq!(sym_payload.len(), 120);
    let req = fulfill::Request {
        payload: sym_payload,
    };
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest("Invalid hexadecimal payload"))
    ));
}

#[test]
fn fulfill_payload_case_insensitivity() {
    // 1. Uppercase hexadecimal characters (120 chars)
    let upper_payload = "AB".repeat(60);
    assert_eq!(upper_payload.len(), 120);
    let req = fulfill::Request {
        payload: upper_payload,
    };
    let res = req.parse_payload();
    assert!(res.is_ok());
    let bytes = res.unwrap();
    assert_eq!(bytes.len(), 60);
    assert_eq!(bytes, vec![0xab; 60]);

    // 2. Mixed-case hexadecimal characters (120 chars)
    let mixed_payload = "aBcD".repeat(30);
    assert_eq!(mixed_payload.len(), 120);
    let req = fulfill::Request {
        payload: mixed_payload,
    };
    let res = req.parse_payload();
    assert!(res.is_ok());
    let bytes = res.unwrap();
    assert_eq!(bytes.len(), 60);
    assert_eq!(bytes, [0xab, 0xcd].repeat(30));
}

#[test]
fn fulfill_payload_boundary_odd_lengths() {
    // 1. 119 chars: under lower boundary (checked by len first)
    let under_odd = "ab".repeat(59) + "a";
    assert_eq!(under_odd.len(), 119);
    let req = fulfill::Request { payload: under_odd };
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest(
            "Payload must be between 120 and 8192 hex chars (or 60 to 4096 bytes)"
        ))
    ));

    // 2. 8191 chars: within length boundary [120, 8192], but odd length
    let upper_odd = "cd".repeat(4095) + "c";
    assert_eq!(upper_odd.len(), 8191);
    let req = fulfill::Request { payload: upper_odd };
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest("Invalid hexadecimal payload"))
    ));

    // 3. 8193 chars: over upper boundary (checked by len first)
    let over_odd = "ef".repeat(4096) + "e";
    assert_eq!(over_odd.len(), 8193);
    let req = fulfill::Request { payload: over_odd };
    let res = req.parse_payload();
    assert!(matches!(
        res,
        Err(BadRequest(
            "Payload must be between 120 and 8192 hex chars (or 60 to 4096 bytes)"
        ))
    ));
}

#[test]
fn fulfill_payload_whitespace_and_invalid_chars() {
    // 1. 120 chars containing spaces
    let mut spaced = "aa".repeat(59);
    spaced.push_str("  ");
    assert_eq!(spaced.len(), 120);
    let req = fulfill::Request { payload: spaced };
    assert!(matches!(
        req.parse_payload(),
        Err(BadRequest("Invalid hexadecimal payload"))
    ));

    // 2. 120 chars containing newlines
    let mut newlined = "aa".repeat(59);
    newlined.push_str("\n\n");
    assert_eq!(newlined.len(), 120);
    let req = fulfill::Request { payload: newlined };
    assert!(matches!(
        req.parse_payload(),
        Err(BadRequest("Invalid hexadecimal payload"))
    ));
}

#[test]
fn create_response_serialization_and_roundtrip() {
    let resp = create::Response {
        mailbox_id: "01234567".to_string(),
        expires_in: 300,
    };

    let serialized = serde_json::to_value(&resp).unwrap();
    assert_eq!(
        serialized,
        json!({
            "mailbox_id": "01234567",
            "expires_in": 300
        })
    );

    // Bidirectional roundtrip deserialization
    let deserialized: create::Response =
        serde_json::from_value(serialized).unwrap();
    assert_eq!(deserialized, resp);
}

#[test]
fn pickup_response_serialization_and_roundtrip() {
    // Pending response: {"status": "pending"}
    let pending_resp = pickup::Response::Pending;
    let pending_json = serde_json::to_value(&pending_resp).unwrap();
    assert_eq!(
        pending_json,
        json!({
            "status": "pending"
        })
    );
    let pending_roundtrip: pickup::Response =
        serde_json::from_value(pending_json).unwrap();
    assert_eq!(pending_roundtrip, pending_resp);

    // Fulfilled response: {"status": "fulfilled", "payload": "<hex>"}
    let fulfilled_resp = pickup::Response::Fulfilled {
        payload: "deadbeefcafebabe".to_string(),
    };
    let fulfilled_json = serde_json::to_value(&fulfilled_resp).unwrap();
    assert_eq!(
        fulfilled_json,
        json!({
            "status": "fulfilled",
            "payload": "deadbeefcafebabe"
        })
    );
    let fulfilled_roundtrip: pickup::Response =
        serde_json::from_value(fulfilled_json).unwrap();
    assert_eq!(fulfilled_roundtrip, fulfilled_resp);
}

#[test]
fn fulfill_request_deserialization_and_roundtrip() {
    let payload_str = "ab".repeat(64);
    let req = fulfill::Request {
        payload: payload_str.clone(),
    };

    let serialized = serde_json::to_value(&req).unwrap();
    assert_eq!(
        serialized,
        json!({
            "payload": payload_str
        })
    );

    let parsed: fulfill::Request = serde_json::from_value(serialized).unwrap();
    assert_eq!(parsed, req);
    assert_eq!(parsed.payload, payload_str);
    let decoded = parsed.parse_payload().unwrap();
    assert_eq!(decoded, vec![0xab; 64]);
}

#[test]
fn fulfill_request_invalid_json_types() {
    // Missing payload field
    assert!(serde_json::from_value::<fulfill::Request>(json!({})).is_err());

    // Payload as integer
    assert!(
        serde_json::from_value::<fulfill::Request>(json!({ "payload": 12345 }))
            .is_err()
    );

    // Payload as null
    assert!(
        serde_json::from_value::<fulfill::Request>(json!({ "payload": null }))
            .is_err()
    );
}
