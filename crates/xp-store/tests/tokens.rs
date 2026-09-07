//! Token indexing (schema v2): mint metadata, holder bookkeeping and burns.
//!
//! Fixture 453051 carries mainnet's SigUSD mint (tx `695f7249…`, whose first input box id
//! `03faf2cb…` *is* the token id), so the mint rule, the EIP-4 metadata read and the holder
//! tables can all be asserted against real chain data. Burns and holder transitions have no
//! fixture, so they are exercised with synthetic blocks built on top of 453051.

use xp_store::keys::{k_by_count, k_hash_gidx, k_token_holder, k_token_tree, k_u64};
use xp_store::rows::TokenRow;
use xp_store::tables::{
    TOKENS, TOKENS_BY_GIDX, TOKENS_BY_HOLDERS, TOKEN_BOXES, TOKEN_HOLDERS, TOKEN_HOLDER_AMT,
    TOKEN_UNSPENT,
};
use xp_store::Store;
use xp_types::{BoxId, Hash32, HeaderId, TxId};
use xp_wire::{decode_block, DecodedBlock, DecodedTx};

fn fixture(h: u32) -> DecodedBlock {
    decode_block(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/{h}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}

fn hash32(hex_str: &str) -> Hash32 {
    let mut h = [0u8; 32];
    h.copy_from_slice(&hex::decode(hex_str).unwrap());
    h
}

/// The SigUSD token id — equal to the id of the mint transaction's first input box.
fn sigusd() -> Hash32 {
    hash32("03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04")
}

const SIGUSD_EMISSION: u64 = 10_000_000_000_001;

/// A store seeded just below the SigUSD mint block, with 453051 already applied.
fn store_with_453051(dir: &std::path::Path) -> Store {
    let s = Store::open(&dir.join("x.redb")).unwrap();
    let b = fixture(453051);
    s.seed_for_tests(453050, b.header.parent_id.0).unwrap();
    s.apply_batch(&[b], true).unwrap();
    s
}

fn token_row(s: &Store, id: &Hash32) -> TokenRow {
    let txn = s.begin_read().unwrap();
    let t = txn.open_table(TOKENS).unwrap();
    let v = t.get(id.as_slice()).unwrap().expect("token row indexed");
    TokenRow::decode(v.value()).unwrap()
}

fn holder_amount(s: &Store, id: &Hash32, tree: &Hash32) -> Option<u64> {
    let txn = s.begin_read().unwrap();
    let t = txn.open_table(TOKEN_HOLDER_AMT).unwrap();
    t.get(k_token_tree(id, tree).as_slice())
        .unwrap()
        .map(|v| u64::from_be_bytes(v.value().try_into().unwrap()))
}

fn has_key(s: &Store, tbl: xp_store::tables::Tbl, k: &[u8]) -> bool {
    let txn = s.begin_read().unwrap();
    txn.open_table(tbl).unwrap().get(k).unwrap().is_some()
}

fn count_prefix(s: &Store, tbl: xp_store::tables::Tbl, prefix: &[u8]) -> usize {
    let txn = s.begin_read().unwrap();
    let (lo, hi) = xp_store::keys::prefix_range(prefix);
    txn.open_table(tbl)
        .unwrap()
        .range(lo.as_slice()..hi.as_slice())
        .unwrap()
        .count()
}

/// The mint tx creates a `TokenRow` from EIP-4 registers, indexes it by mint gidx and holder
/// count, and records the mint output's tree as the token's first holder.
#[test]
fn sigusd_mint_is_indexed_with_metadata_and_holder() {
    let dir = tempfile::tempdir().unwrap();
    let s = store_with_453051(dir.path());
    let id = sigusd();

    let b = fixture(453051);
    let mint_tx = &b.txs[1];
    let mint_box = &mint_tx.outputs[0];
    assert!(
        mint_box.tokens.iter().any(|(t, _)| *t == id),
        "fixture's mint output must carry the token"
    );

    let row = token_row(&s, &id);
    assert_eq!(row.name, "SigUSD");
    assert_eq!(row.description, "SigmaUSD - V2");
    assert_eq!(row.decimals, Some(2));
    assert_eq!(row.emission, SIGUSD_EMISSION);
    assert_eq!(row.burned, 0);
    assert_eq!(row.mint_tx, mint_tx.id.0);
    assert_eq!(row.mint_box, mint_box.id.0);
    assert_eq!(row.mint_height, 453051);
    assert!(row.box_count >= 1);
    assert!(row.holder_count >= 1);

    // Mint gidx points at the mint box and is the key of the newest-first token listing.
    let rd = xp_store::Reader::new(&s).unwrap();
    let gidx = rd
        .box_by_id(&mint_box.id.0)
        .unwrap()
        .expect("mint box indexed")
        .gidx;
    assert_eq!(row.mint_gidx, gidx);
    let txn = s.begin_read().unwrap();
    let by_gidx = txn.open_table(TOKENS_BY_GIDX).unwrap();
    assert_eq!(
        by_gidx
            .get(k_u64(gidx).as_slice())
            .unwrap()
            .expect("TOKENS_BY_GIDX entry")
            .value(),
        id.as_slice()
    );
    drop(txn);

    assert!(has_key(
        &s,
        TOKENS_BY_HOLDERS,
        k_by_count(row.holder_count, &id).as_slice()
    ));
    assert!(has_key(&s, TOKEN_BOXES, k_hash_gidx(&id, gidx).as_slice()));
    assert!(has_key(
        &s,
        TOKEN_UNSPENT,
        k_hash_gidx(&id, gidx).as_slice()
    ));

    // The mint output's tree holds the whole emission.
    let tree = mint_box.tree_hash.0;
    assert_eq!(holder_amount(&s, &id, &tree), Some(SIGUSD_EMISSION));
    assert!(has_key(
        &s,
        TOKEN_HOLDERS,
        k_token_holder(&id, SIGUSD_EMISSION, &tree).as_slice()
    ));
}

/// A synthetic block on top of 453051 whose only tx spends the mint box and outputs less of
/// the token than it consumed: the difference is burned and the holder's amount drops.
#[test]
fn spending_less_than_was_consumed_burns_the_difference() {
    let dir = tempfile::tempdir().unwrap();
    let s = store_with_453051(dir.path());
    let id = sigusd();
    let b = fixture(453051);
    let mint_box = b.txs[1].outputs[0].clone();

    let mut out = mint_box.clone();
    out.id = BoxId([0x77u8; 32]);
    out.tx_id = TxId([0xDDu8; 32]);
    out.index = 0;
    out.tokens = vec![(id, 1)];

    let mut b2 = fixture(453051);
    b2.header.height = 453052;
    b2.header.parent_id = b.header.id;
    b2.header.id = HeaderId([0xC2u8; 32]);
    b2.txs = vec![DecodedTx {
        id: TxId([0xDDu8; 32]),
        inputs: vec![mint_box.id],
        data_inputs: vec![],
        outputs: vec![out.clone()],
        size: 100,
    }];
    s.apply_batch(&[b2], true).unwrap();

    let row = token_row(&s, &id);
    assert_eq!(row.burned, SIGUSD_EMISSION - 1);
    assert_eq!(row.emission, SIGUSD_EMISSION);
    assert_eq!(row.box_count, 2);
    // Same tree, so still exactly one holder — now holding what survived the burn.
    assert_eq!(row.holder_count, 1);
    let tree = mint_box.tree_hash.0;
    assert_eq!(holder_amount(&s, &id, &tree), Some(1));
    assert!(has_key(
        &s,
        TOKEN_HOLDERS,
        k_token_holder(&id, 1, &tree).as_slice()
    ));
    assert!(!has_key(
        &s,
        TOKEN_HOLDERS,
        k_token_holder(&id, SIGUSD_EMISSION, &tree).as_slice()
    ));
}

/// Holder-count transitions inside a single block: tx A moves the whole supply to a tree that
/// held none of it (0 → 1 for that tree), tx B spends that box and outputs no tokens at all,
/// burning the supply and taking every holder back to 0.
#[test]
fn holder_count_goes_zero_to_one_to_zero_within_a_block() {
    let dir = tempfile::tempdir().unwrap();
    let s = store_with_453051(dir.path());
    let id = sigusd();
    let b = fixture(453051);
    let mint_box = b.txs[1].outputs[0].clone();
    let other = b.txs[1].outputs[1].clone();
    assert_ne!(
        mint_box.tree_hash.0, other.tree_hash.0,
        "need a second, distinct tree"
    );
    assert!(!other.tokens.iter().any(|(t, _)| *t == id));

    // A: the whole supply moves to `other`'s tree.
    let mut a_out = other.clone();
    a_out.id = BoxId([0xA1u8; 32]);
    a_out.tx_id = TxId([0xAAu8; 32]);
    a_out.index = 0;
    a_out.value = mint_box.value;
    a_out.tokens = vec![(id, SIGUSD_EMISSION)];
    let tx_a = DecodedTx {
        id: TxId([0xAAu8; 32]),
        inputs: vec![mint_box.id],
        data_inputs: vec![],
        outputs: vec![a_out.clone()],
        size: 100,
    };
    // B: spends A's box and keeps none of the token.
    let mut b_out = a_out.clone();
    b_out.id = BoxId([0xB1u8; 32]);
    b_out.tx_id = TxId([0xBBu8; 32]);
    b_out.tokens = vec![];
    let tx_b = DecodedTx {
        id: TxId([0xBBu8; 32]),
        inputs: vec![a_out.id],
        data_inputs: vec![],
        outputs: vec![b_out],
        size: 100,
    };

    let mut b2 = fixture(453051);
    b2.header.height = 453052;
    b2.header.parent_id = b.header.id;
    b2.header.id = HeaderId([0xC3u8; 32]);
    b2.txs = vec![tx_a, tx_b];
    s.apply_batch(&[b2], true).unwrap();

    let row = token_row(&s, &id);
    assert_eq!(row.holder_count, 0);
    assert_eq!(row.burned, SIGUSD_EMISSION);
    assert_eq!(row.box_count, 2); // mint box + A's output; B's output carries none
    assert_eq!(holder_amount(&s, &id, &mint_box.tree_hash.0), None);
    assert_eq!(holder_amount(&s, &id, &other.tree_hash.0), None);
    assert_eq!(count_prefix(&s, TOKEN_HOLDERS, id.as_slice()), 0);
    assert_eq!(count_prefix(&s, TOKEN_UNSPENT, id.as_slice()), 0);
    assert_eq!(count_prefix(&s, TOKEN_BOXES, id.as_slice()), 2);
    assert!(has_key(
        &s,
        TOKENS_BY_HOLDERS,
        k_by_count(0, &id).as_slice()
    ));
    assert!(!has_key(
        &s,
        TOKENS_BY_HOLDERS,
        k_by_count(1, &id).as_slice()
    ));
}

/// Rolling back a block that *mutated* an existing token — rather than minting one — must be
/// an identity too. This is the path the mint fixture cannot reach: `prev_tokens` (a restored
/// `TokenRow` with its burn and holder counters) and `prev_holder_amts` with a `Some` previous
/// amount, both re-keying `TOKENS_BY_HOLDERS`/`TOKEN_HOLDERS` on the way back.
///
/// The block deliberately splits the supply across *two* trees while burning the rest, so the
/// token's `holder_count` moves 1 → 2. That is what exercises the `prev_tokens` re-key branch
/// of rollback: `TOKENS_BY_HOLDERS` must lose the `(2, id)` key it gained and get `(1, id)`
/// back, which a same-count rollback would never notice.
#[test]
fn rolling_back_a_burn_and_a_new_holder_restores_the_token_row_and_its_keys() {
    let dir = tempfile::tempdir().unwrap();
    let s = store_with_453051(dir.path());
    let id = sigusd();
    let before = s.fingerprint().unwrap();
    let b = fixture(453051);
    let mint_box = b.txs[1].outputs[0].clone();
    let other = b.txs[1].outputs[1].clone();
    let t1 = mint_box.tree_hash.0;
    let t2 = other.tree_hash.0;
    assert_ne!(t1, t2, "need a second, distinct tree");
    assert_eq!(token_row(&s, &id).holder_count, 1);

    // One output keeps the mint tree a holder, one makes a second tree one, and the rest of
    // the supply is burned.
    let mut keep = mint_box.clone();
    keep.id = BoxId([0x77u8; 32]);
    keep.tx_id = TxId([0xDDu8; 32]);
    keep.index = 0;
    keep.value = mint_box.value / 2;
    keep.tokens = vec![(id, 1)];
    let mut moved = other.clone();
    moved.id = BoxId([0x78u8; 32]);
    moved.tx_id = TxId([0xDDu8; 32]);
    moved.index = 1;
    moved.value = mint_box.value - keep.value;
    moved.tokens = vec![(id, 1)];

    let mut b2 = fixture(453051);
    b2.header.height = 453052;
    b2.header.parent_id = b.header.id;
    b2.header.id = HeaderId([0xC4u8; 32]);
    b2.txs = vec![DecodedTx {
        id: TxId([0xDDu8; 32]),
        inputs: vec![mint_box.id],
        data_inputs: vec![],
        outputs: vec![keep, moved],
        size: 100,
    }];

    s.apply_batch(&[b2], true).unwrap();
    assert_ne!(s.fingerprint().unwrap(), before);
    let applied = token_row(&s, &id);
    assert_eq!(applied.burned, SIGUSD_EMISSION - 2);
    assert_eq!(applied.holder_count, 2);
    assert_eq!(holder_amount(&s, &id, &t1), Some(1));
    assert_eq!(holder_amount(&s, &id, &t2), Some(1));
    assert!(has_key(
        &s,
        TOKENS_BY_HOLDERS,
        k_by_count(2, &id).as_slice()
    ));
    assert!(!has_key(
        &s,
        TOKENS_BY_HOLDERS,
        k_by_count(1, &id).as_slice()
    ));

    s.rollback_to(453051).unwrap();
    assert_eq!(s.fingerprint().unwrap(), before);
    let restored = token_row(&s, &id);
    assert_eq!(restored.burned, 0);
    assert_eq!(restored.holder_count, 1);
    assert_eq!(holder_amount(&s, &id, &t1), Some(SIGUSD_EMISSION));
    assert_eq!(holder_amount(&s, &id, &t2), None);
    // Only the original holder-count key survives the rollback.
    assert!(has_key(
        &s,
        TOKENS_BY_HOLDERS,
        k_by_count(1, &id).as_slice()
    ));
    assert!(!has_key(
        &s,
        TOKENS_BY_HOLDERS,
        k_by_count(2, &id).as_slice()
    ));
    assert_eq!(count_prefix(&s, TOKEN_HOLDERS, id.as_slice()), 1);
    assert!(has_key(
        &s,
        TOKEN_HOLDERS,
        k_token_holder(&id, SIGUSD_EMISSION, &t1).as_slice()
    ));
}
