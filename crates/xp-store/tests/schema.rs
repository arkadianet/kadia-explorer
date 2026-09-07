//! The schema-version guard on `Store::open`.
//!
//! `SCHEMA_VERSION = 2` is a Global Constraint of the tokens/templates plan: a v1 store has
//! none of the eleven schema-v2 tables populated, so opening one would serve empty token,
//! holder and template pages forever without a single error in the log. The guard that
//! prevents that is one `match` arm, and this file is what stops a later "simplification"
//! from deleting it.

use redb::Database;
use xp_store::keys::k_u32;
use xp_store::tables::{META, META_SCHEMA, SCHEMA_VERSION};
use xp_store::{Store, StoreError};

/// Rewrites `META_SCHEMA` to `v` through the redb API, behind the store's back.
fn force_schema_version(path: &std::path::Path, v: u32) {
    let db = Database::open(path).unwrap();
    let txn = db.begin_write().unwrap();
    {
        let mut meta = txn.open_table(META).unwrap();
        meta.insert(META_SCHEMA, k_u32(v).as_slice()).unwrap();
    }
    txn.commit().unwrap();
}

/// Reads `META_SCHEMA` back out through the redb API.
fn schema_version(path: &std::path::Path) -> Option<u32> {
    let db = Database::open(path).unwrap();
    let txn = db.begin_read().unwrap();
    let meta = txn.open_table(META).unwrap();
    let v = meta.get(META_SCHEMA).unwrap()?;
    Some(u32::from_be_bytes(v.value().try_into().unwrap()))
}

/// A fresh store stamps the current schema version, and reopening it is accepted.
#[test]
fn a_fresh_store_records_the_current_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    drop(Store::open(&path).unwrap());

    assert_eq!(SCHEMA_VERSION, 2, "the tokens plan's schema version");
    assert_eq!(schema_version(&path), Some(2));
    // Reopening an already-stamped store takes the `Some(v) if v == SCHEMA_VERSION` arm.
    Store::open(&path).unwrap();
}

/// A store stamped with schema v1 — one written before the token/template tables existed —
/// is refused rather than opened with eleven silently empty tables.
#[test]
fn a_schema_v1_store_is_refused_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    drop(Store::open(&path).unwrap());
    force_schema_version(&path, 1);
    assert_eq!(schema_version(&path), Some(1), "the v1 store is set up");

    let err = match Store::open(&path) {
        Ok(_) => panic!("expected Store::open to refuse a schema-v1 store"),
        Err(e) => e,
    };
    assert!(
        matches!(err, StoreError::Corrupt(m) if m.contains("schema version mismatch")),
        "unexpected error: {err}"
    );

    // The refusal is not a mutation: the write transaction that (re)creates the tables is
    // dropped without committing, so the version on disk is still the one we wrote.
    assert_eq!(schema_version(&path), Some(1));
}
