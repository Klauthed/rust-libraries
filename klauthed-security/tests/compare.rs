//! Public-API integration tests for `klauthed_security::compare`.

use klauthed_security::compare::*;

#[test]
fn equal_slices_match() {
    assert!(constant_time_eq(b"hello world", b"hello world"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn differing_slices_do_not_match() {
    assert!(!constant_time_eq(b"hello world", b"hello worle"));
}

#[test]
fn different_lengths_do_not_match() {
    assert!(!constant_time_eq(b"abc", b"abcd"));
}
