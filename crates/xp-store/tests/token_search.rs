use redb::{Database, ReadableTable};
use std::path::Path;
use xp_store::rows::TokenRow;
use xp_store::tables::*;
use xp_store::token_search::*;
use xp_store::{Reader, Store, StoreError};
use xp_types::{BoxId, Hash32, HeaderId, TxId};
use xp_wire::{decode_block, DecodedBlock, DecodedTx};

fn row(name: &str) -> TokenRow {
    TokenRow {
        mint_tx: [1; 32],
        mint_box: [2; 32],
        mint_height: 1,
        mint_gidx: 0,
        name: name.into(),
        description: "Original metadata".into(),
        decimals: Some(0),
        token_type: None,
        emission: 10,
        burned: 0,
        holder_count: 1,
        box_count: 1,
    }
}

fn edit_db(path: &Path, edit: impl FnOnce(&redb::WriteTransaction)) {
    let db = Database::open(path).unwrap();
    let txn = db.begin_write().unwrap();
    edit(&txn);
    txn.commit().unwrap();
}

fn old_tokens(path: &Path, names: &[(u8, &str)]) {
    edit_db(path, |txn| {
        let mut table = txn.open_table(TOKENS).unwrap();
        for (id, name) in names {
            table
                .insert([*id; 32].as_slice(), row(name).encode().as_slice())
                .unwrap();
        }
    });
}

fn finish(store: &Store, batch: usize) -> TokenNameIndexStatus {
    for _ in 0..100 {
        let status = store.backfill_token_names_batch(batch).unwrap();
        if status.ready {
            return status;
        }
    }
    panic!("backfill did not terminate")
}

fn names(store: &Store, query: &str, mode: TokenNameMatch) -> Vec<Hash32> {
    Reader::new(store)
        .unwrap()
        .search_token_names(query, mode, None, 100)
        .unwrap()
        .items
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

fn fixture() -> DecodedBlock {
    decode_block(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/453051.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}

fn mint(base: &DecodedBlock, height: u32, parent: HeaderId, id: u8, name: &str) -> DecodedBlock {
    assert!(name.len() < 128);
    let mut block = base.clone();
    block.header.height = height;
    block.header.parent_id = parent;
    block.header.id = HeaderId([id.wrapping_add(100); 32]);
    let mut output = base.txs[1].outputs[0].clone();
    output.id = BoxId([id.wrapping_add(50); 32]);
    output.tx_id = TxId([id.wrapping_add(60); 32]);
    output.tokens = vec![([id; 32], 10)];
    let encoded = format!("0e{:02x}{}", name.len(), hex::encode(name));
    output.registers_json = serde_json::json!({"R4":encoded}).to_string();
    block.txs = vec![DecodedTx {
        id: output.tx_id,
        inputs: vec![BoxId([id; 32])],
        data_inputs: vec![],
        outputs: vec![output],
        size: 100,
    }];
    block
}

#[test]
fn fresh_store_and_real_mint_are_searchable_without_backfill() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("x.redb")).unwrap();
    assert!(
        Reader::new(&store)
            .unwrap()
            .token_name_index_status()
            .unwrap()
            .ready
    );
    let block = fixture();
    store
        .seed_for_tests(453050, block.header.parent_id.0)
        .unwrap();
    let before = store.fingerprint().unwrap();
    store.apply_batch(&[block], true).unwrap();
    let result = Reader::new(&store)
        .unwrap()
        .search_token_names("  SIGusd  ", TokenNameMatch::Exact, None, 10)
        .unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].1.name, "SigUSD");
    assert_eq!(result.normalized_query, "sigusd");
    store.rollback_to(453050).unwrap();
    assert!(names(&store, "sig", TokenNameMatch::Prefix).is_empty());
    assert_eq!(store.fingerprint().unwrap(), before);
}

#[test]
fn populated_legacy_store_backfills_in_batches_and_resumes_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    drop(Store::open(&path).unwrap());
    old_tokens(&path, &[(1, "Alpha"), (2, "Beta"), (3, "Gamma")]);
    let store = Store::open(&path).unwrap();
    let before = store.fingerprint().unwrap();
    assert!(matches!(
        Reader::new(&store)
            .unwrap()
            .search_token_names("alp", TokenNameMatch::Prefix, None, 10),
        Err(StoreError::TokenSearchNotReady)
    ));
    let first = store.backfill_token_names_batch(1).unwrap();
    assert!(!first.ready);
    assert_eq!(first.scanned_tokens, 1);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        Reader::new(&store)
            .unwrap()
            .token_name_index_status()
            .unwrap()
            .scanned_tokens,
        1
    );
    let status = finish(&store, 1);
    assert_eq!(status.scanned_tokens, 3);
    assert_eq!(status.indexed_names, 3);
    assert_eq!(names(&store, "alpha", TokenNameMatch::Exact), vec![[1; 32]]);
    assert_eq!(store.fingerprint().unwrap(), before);
}

#[test]
fn colliding_names_paginate_stably_without_conflating_token_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    drop(Store::open(&path).unwrap());
    old_tokens(
        &path,
        &[
            (8, " SigUSD "),
            (2, "SIGUSD"),
            (4, "SigUSD Plus"),
            (9, "ΣIGUSD"),
            (10, "ÉRGO"),
            (11, "érgo"),
            (12, "e\u{301}rgo"),
        ],
    );
    let store = Store::open(&path).unwrap();
    finish(&store, 2);
    let reader = Reader::new(&store).unwrap();
    let first = reader
        .search_token_names("sigusd", TokenNameMatch::Prefix, None, 1)
        .unwrap();
    assert_eq!(first.items[0].0, [2; 32]);
    let second = reader
        .search_token_names("sigusd", TokenNameMatch::Prefix, first.next_cursor, 1)
        .unwrap();
    assert_eq!(second.items[0].0, [8; 32]);
    let third = reader
        .search_token_names("sigusd", TokenNameMatch::Prefix, second.next_cursor, 1)
        .unwrap();
    assert_eq!(third.items[0].0, [4; 32]);
    assert_eq!(third.next_cursor, None);
    assert_eq!(
        names(&store, "sigusd", TokenNameMatch::Exact),
        vec![[2; 32], [8; 32]]
    );
    assert_eq!(
        names(&store, "ΣIGUSD", TokenNameMatch::Exact),
        vec![[9; 32]]
    );
    assert_eq!(names(&store, "ÉRGO", TokenNameMatch::Exact), vec![[10; 32]]);
    assert_eq!(names(&store, "érgo", TokenNameMatch::Exact), vec![[11; 32]]);
    assert_eq!(
        names(&store, "e\u{301}rgo", TokenNameMatch::Exact),
        vec![[12; 32]]
    );
    assert!(matches!(
        reader.search_token_names("other", TokenNameMatch::Exact, Some([2; 32]), 10),
        Err(StoreError::InvalidTokenSearch(_))
    ));
    assert!(matches!(
        reader.search_token_names("sig", TokenNameMatch::Prefix, Some([99; 32]), 10),
        Err(StoreError::InvalidTokenSearch(_))
    ));
}

#[test]
fn exclusions_and_request_limits_are_explicit_and_preserve_original_names() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    drop(Store::open(&path).unwrap());
    let longest = "x".repeat(MAX_TOKEN_NAME_BYTES);
    let too_long = "x".repeat(MAX_TOKEN_NAME_BYTES + 1);
    old_tokens(
        &path,
        &[
            (1, &longest),
            (2, &too_long),
            (3, ""),
            (4, "bad\0name"),
            (5, "A\t \nB"),
        ],
    );
    let store = Store::open(&path).unwrap();
    let status = finish(&store, 2);
    assert_eq!(status.indexed_names, 2);
    assert_eq!(status.unindexed_tokens, 3);
    assert_eq!(names(&store, "xxx", TokenNameMatch::Prefix), vec![[1; 32]]);
    assert_eq!(
        names(&store, " a  B ", TokenNameMatch::Exact),
        vec![[5; 32]]
    );
    assert_eq!(
        Reader::new(&store)
            .unwrap()
            .token(&[2; 32])
            .unwrap()
            .unwrap()
            .name,
        too_long
    );
    for invalid in ["".into(), " \r\n".into(), "a\0b".into(), "é".repeat(257)] {
        assert!(matches!(
            normalize_token_name(&invalid),
            Err(StoreError::InvalidTokenSearch(_))
        ));
    }
    assert!(store.backfill_token_names_batch(0).is_err());
    assert!(store.backfill_token_names_batch(1001).is_err());
    assert!(Reader::new(&store)
        .unwrap()
        .search_token_names("a", TokenNameMatch::Prefix, None, 101)
        .is_err());
}

#[test]
fn building_index_tracks_mints_before_cursor_and_rollback_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    let base = fixture();
    let store = Store::open(&path).unwrap();
    store
        .seed_for_tests(453050, base.header.parent_id.0)
        .unwrap();
    drop(store);
    old_tokens(&path, &[(80, "Before"), (90, "After")]);
    let store = Store::open(&path).unwrap();
    assert!(!store.backfill_token_names_batch(1).unwrap().ready);
    let keep = mint(&base, 453051, base.header.parent_id, 10, "Keep");
    let remove = mint(&base, 453052, keep.header.id, 11, "Remove");
    store.apply_batch(&[keep, remove], true).unwrap();
    store.rollback_to(453051).unwrap();
    finish(&store, 1);
    assert_eq!(names(&store, "keep", TokenNameMatch::Exact), vec![[10; 32]]);
    assert!(names(&store, "remove", TokenNameMatch::Exact).is_empty());
    assert_eq!(
        names(&store, "after", TokenNameMatch::Exact),
        vec![[90; 32]]
    );
}

#[test]
fn obsolete_version_rebuilds_and_clears_stale_names_in_bounded_batches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    drop(Store::open(&path).unwrap());
    old_tokens(&path, &[(1, "Alpha"), (2, "Beta"), (3, "Gamma")]);
    let store = Store::open(&path).unwrap();
    finish(&store, 3);
    drop(store);
    edit_db(&path, |txn| {
        let mut meta = txn.open_table(TOKEN_NAME_META).unwrap();
        let mut state = meta
            .get(b"state".as_slice())
            .unwrap()
            .unwrap()
            .value()
            .to_vec();
        state[..4].copy_from_slice(&0u32.to_be_bytes());
        meta.insert(b"state".as_slice(), state.as_slice()).unwrap();
    });
    let store = Store::open(&path).unwrap();
    assert_eq!(
        Reader::new(&store)
            .unwrap()
            .token_name_index_status()
            .unwrap()
            .phase,
        "clearing"
    );
    assert_eq!(
        store.backfill_token_names_batch(1).unwrap().phase,
        "clearing"
    );
    let txn = store.begin_read().unwrap();
    assert_eq!(
        txn.open_table(TOKEN_NAMES).unwrap().iter().unwrap().count(),
        2
    );
    drop(txn);
    finish(&store, 1);
    assert_eq!(names(&store, "alpha", TokenNameMatch::Exact), vec![[1; 32]]);
}

#[test]
fn byte_budgets_admit_before_decoding_and_resume_between_large_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    drop(Store::open(&path).unwrap());
    edit_db(&path, |txn| {
        let mut table = txn.open_table(TOKENS).unwrap();
        for id in [1u8, 2] {
            let mut token = row("Large");
            token.description = "x".repeat(5 * 1024 * 1024);
            table
                .insert([id; 32].as_slice(), token.encode().as_slice())
                .unwrap();
        }
    });
    let store = Store::open(&path).unwrap();
    let status = store.backfill_token_names_batch(1000).unwrap();
    assert!(!status.ready);
    assert_eq!(status.scanned_tokens, 1);
    finish(&store, 1000);
    assert!(matches!(
        Reader::new(&store)
            .unwrap()
            .search_token_names("large", TokenNameMatch::Exact, None, 1),
        Err(StoreError::ReadLimit(_))
    ));
    drop(store);
    edit_db(&path, |txn| {
        let mut token = row("Too large");
        token.description = "x".repeat(MAX_TOKEN_NAME_BATCH_BYTES);
        txn.open_table(TOKENS)
            .unwrap()
            .insert([3u8; 32].as_slice(), token.encode().as_slice())
            .unwrap();
    });
    let store = Store::open(&path).unwrap();
    loop {
        match store.backfill_token_names_batch(1) {
            Err(StoreError::ReadLimit(_)) => break,
            Ok(status) => assert!(!status.ready),
            other => panic!("unexpected {other:?}"),
        }
    }
}

type AuxiliarySnapshot = Vec<(Vec<u8>, Vec<u8>)>;

fn auxiliary_snapshot(store: &Store) -> (AuxiliarySnapshot, AuxiliarySnapshot) {
    let txn = store.begin_read().unwrap();
    let read = |table: Tbl| {
        txn.open_table(table)
            .unwrap()
            .iter()
            .unwrap()
            .map(|entry| {
                let (key, value) = entry.unwrap();
                (key.value().to_vec(), value.value().to_vec())
            })
            .collect()
    };
    (read(TOKEN_NAMES), read(TOKEN_NAME_META))
}

fn restore_auxiliary(path: &Path, snapshot: (AuxiliarySnapshot, AuxiliarySnapshot)) {
    edit_db(path, |txn| {
        for (definition, rows) in [(TOKEN_NAMES, snapshot.0), (TOKEN_NAME_META, snapshot.1)] {
            txn.delete_table(definition).unwrap();
            let mut table = txn.open_table(definition).unwrap();
            for (key, value) in rows {
                table.insert(key.as_slice(), value.as_slice()).unwrap();
            }
        }
    });
}

#[test]
fn old_binary_forward_writes_cannot_be_certified_by_the_next_apply() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    let base = fixture();
    let store = Store::open(&path).unwrap();
    store
        .seed_for_tests(453050, base.header.parent_id.0)
        .unwrap();
    let old = auxiliary_snapshot(&store);
    let first = mint(&base, 453051, base.header.parent_id, 10, "Old binary mint");
    let second = mint(&base, 453052, first.header.id, 11, "New binary mint");
    store.apply_batch(&[first], true).unwrap();
    drop(store);
    // Reproduce an old binary applying core rows without touching auxiliary tables.
    restore_auxiliary(&path, old);
    let store = Store::open(&path).unwrap();
    assert!(
        !Reader::new(&store)
            .unwrap()
            .token_name_index_status()
            .unwrap()
            .ready
    );
    store.apply_batch(&[second], true).unwrap();
    assert!(
        !Reader::new(&store)
            .unwrap()
            .token_name_index_status()
            .unwrap()
            .ready
    );
    finish(&store, 1);
    assert_eq!(
        names(&store, "old binary mint", TokenNameMatch::Exact),
        vec![[10; 32]]
    );
    assert_eq!(
        names(&store, "new binary mint", TokenNameMatch::Exact),
        vec![[11; 32]]
    );
}

#[test]
fn old_binary_rollback_and_same_height_fork_invalidate_the_full_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    let base = fixture();
    let store = Store::open(&path).unwrap();
    store
        .seed_for_tests(453050, base.header.parent_id.0)
        .unwrap();
    let first = mint(&base, 453051, base.header.parent_id, 10, "Orphan");
    store.apply_batch(&[first], true).unwrap();
    let stale = auxiliary_snapshot(&store);
    store.rollback_to(453050).unwrap();
    drop(store);
    restore_auxiliary(&path, stale.clone());
    let store = Store::open(&path).unwrap();
    assert_eq!(
        Reader::new(&store)
            .unwrap()
            .token_name_index_status()
            .unwrap()
            .phase,
        "clearing"
    );
    finish(&store, 1);
    assert!(names(&store, "orphan", TokenNameMatch::Exact).is_empty());
    let fork = mint(&base, 453051, base.header.parent_id, 11, "Canonical");
    store.apply_batch(&[fork], true).unwrap();
    drop(store);
    // Same height/token count/counters as stale; only the canonical header ID differs.
    restore_auxiliary(&path, stale);
    let store = Store::open(&path).unwrap();
    assert!(
        !Reader::new(&store)
            .unwrap()
            .token_name_index_status()
            .unwrap()
            .ready
    );
    finish(&store, 1);
    assert!(names(&store, "orphan", TokenNameMatch::Exact).is_empty());
    assert_eq!(
        names(&store, "canonical", TokenNameMatch::Exact),
        vec![[11; 32]]
    );
}

#[test]
fn background_backfill_can_race_mint_and_rollback_without_losing_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    let base = fixture();
    let store = Store::open(&path).unwrap();
    store
        .seed_for_tests(453050, base.header.parent_id.0)
        .unwrap();
    drop(store);
    let legacy: Vec<_> = (80u8..100).map(|id| (id, "Historical")).collect();
    old_tokens(&path, &legacy);
    let store = std::sync::Arc::new(Store::open(&path).unwrap());
    let worker = {
        let store = store.clone();
        std::thread::spawn(move || finish(&store, 1))
    };
    let keep = mint(&base, 453051, base.header.parent_id, 10, "Keep");
    let remove = mint(&base, 453052, keep.header.id, 11, "Remove");
    store.apply_batch(&[keep, remove], true).unwrap();
    store.rollback_to(453051).unwrap();
    worker.join().unwrap();
    assert_eq!(names(&store, "historical", TokenNameMatch::Exact).len(), 20);
    assert_eq!(names(&store, "keep", TokenNameMatch::Exact), vec![[10; 32]]);
    assert!(names(&store, "remove", TokenNameMatch::Exact).is_empty());
}

#[test]
fn headerless_partial_store_opens_without_inventing_a_name_index_anchor() {
    for partial_from in [20u32, 21] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        drop(Store::open(&path).unwrap());
        edit_db(&path, |txn| {
            let mut meta = txn.open_table(META).unwrap();
            meta.insert(META_INDEXED_HEIGHT, xp_store::keys::k_u32(20).as_slice())
                .unwrap();
            meta.insert(
                META_PARTIAL_FROM,
                xp_store::keys::k_u32(partial_from).as_slice(),
            )
            .unwrap();
        });
        let store = Store::open(&path).unwrap();
        let before = store.fingerprint().unwrap();
        let reader = Reader::new(&store).unwrap();
        let status = reader.token_name_index_status().unwrap();
        assert!(!status.ready);
        assert_eq!(status.phase, "stale");
        assert!(matches!(
            reader.search_token_names("name", TokenNameMatch::Exact, None, 10),
            Err(StoreError::TokenSearchNotReady)
        ));
        assert!(matches!(
            store.backfill_token_names_batch(10),
            Err(StoreError::TokenSearchNotReady)
        ));
        // The canonical writer retains its preexisting missing-header integrity check.
        let base = fixture();
        let block = mint(&base, 21, HeaderId([0; 32]), 10, "Name");
        assert!(matches!(
            store.apply_batch(&[block], true),
            Err(StoreError::Corrupt("missing tip header"))
        ));
        assert_eq!(store.fingerprint().unwrap(), before);
        drop(reader);
        store.seed_for_tests(20, [8; 32]).unwrap();
        assert!(
            Reader::new(&store)
                .unwrap()
                .token_name_index_status()
                .unwrap()
                .ready
        );
    }
}
