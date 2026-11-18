use axum::http::{header::AUTHORIZATION, HeaderMap};
use subtle::ConstantTimeEq;

/// `Authorization: Bearer <token>` compared in constant time (after the length check).
pub fn bearer_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(value) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return false;
    };
    token.len() == expected.len() && bool::from(token.as_bytes().ct_eq(expected.as_bytes()))
}
