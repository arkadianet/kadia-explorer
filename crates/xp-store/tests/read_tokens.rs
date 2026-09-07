//! `Reader` snapshot queries over the schema-v2 token, template and register indexes.
//!
//! Two fixture sets are used, matching the tables under test: 453051 (seeded at 453050)
//! carries mainnet's SigUSD mint, so token rows, holders and token boxes can be asserted
//! against real chain data; the 1866000..=1866002 trio is the general-purpose set used by
//! `tests/apply.rs` and carries the shared script templates and R4 registers.

use std::collections::{HashMap, HashSet};
use xp_store::read::Dir;
use xp_store::{Reader, Store};
use xp_types::{BoxId, Hash32, HeaderId, TxId};
use xp_wire::{DecodedBlock, DecodedBox, DecodedTx};

fn fixture(h: u32) -> DecodedBlock {
    xp_wire::decode_block(
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

/// A store carrying the three general-purpose fixture blocks.
fn store_with_1866000() -> (tempfile::TempDir, Store, [DecodedBlock; 3]) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    let b0 = fixture(1866000);
    s.seed_for_tests(1865999, b0.header.parent_id.0).unwrap();
    let blocks = [fixture(1866000), fixture(1866001), fixture(1866002)];
    s.apply_batch(&blocks, true).unwrap();
    (dir, s, blocks)
}

fn all_outputs(blocks: &[DecodedBlock]) -> Vec<DecodedBox> {
    blocks
        .iter()
        .flat_map(|b| b.txs.iter())
        .flat_map(|t| t.outputs.iter())
        .cloned()
        .collect()
}

/// The three distinct-tree fixture outputs used as prototypes for synthetic boxes (their
/// trees are already indexed, so a synthetic output can reuse one without inventing a tree).
fn distinct_tree_outputs(b: &DecodedBlock) -> Vec<DecodedBox> {
    let mut trees: Vec<DecodedBox> = Vec::new();
    for o in all_outputs(std::slice::from_ref(b)) {
        if !trees.iter().any(|t| t.tree_hash.0 == o.tree_hash.0) {
            trees.push(o);
        }
        if trees.len() == 3 {
            break;
        }
    }
    assert_eq!(trees.len(), 3, "need three distinct trees in the fixture");
    trees
}

/// 453051 plus a synthetic block that spends the SigUSD mint box into two outputs on two
/// distinct trees (both carrying SigUSD, so it ends with 2 holders) while minting a second
/// token on the first of them (1 holder). Gives the token listings more than one row, with
/// distinct holder counts and mint gidxs, and SigUSD both spent and unspent boxes.
///
/// Returns the store, the new token's id and the two synthetic box ids in output order.
fn store_with_two_tokens(dir: &std::path::Path) -> (Store, Hash32, [Hash32; 2]) {
    let s = store_with_453051(dir);
    let b = fixture(453051);
    let mint_box = b.txs[1].outputs[0].clone();
    // Ergo's mint rule: the minted token's id is the id of the tx's first input box.
    let new_token = mint_box.id.0;
    let protos = distinct_tree_outputs(&b);

    let mut o0 = protos[0].clone();
    o0.id = BoxId([0xE0u8; 32]);
    o0.tx_id = TxId([0xDDu8; 32]);
    o0.index = 0;
    o0.value = 1_000_000;
    o0.tokens = vec![(sigusd(), 3), (new_token, 7)];
    let mut o1 = protos[1].clone();
    o1.id = BoxId([0xE1u8; 32]);
    o1.tx_id = TxId([0xDDu8; 32]);
    o1.index = 1;
    o1.value = 1_000_000;
    o1.tokens = vec![(sigusd(), 2)];

    let mut b2 = fixture(453051);
    b2.header.height = 453052;
    b2.header.parent_id = b.header.id;
    b2.header.id = HeaderId([0xCAu8; 32]);
    b2.txs = vec![DecodedTx {
        id: TxId([0xDDu8; 32]),
        inputs: vec![mint_box.id],
        data_inputs: vec![],
        outputs: vec![o0.clone(), o1.clone()],
        size: 100,
    }];
    s.apply_batch(&[b2], true).unwrap();
    (s, new_token, [o0.id.0, o1.id.0])
}

#[test]
fn token_returns_the_mint_row_and_none_for_an_unknown_id() {
    let dir = tempfile::tempdir().unwrap();
    let s = store_with_453051(dir.path());
    let rd = Reader::new(&s).unwrap();

    let row = rd.token(&sigusd()).unwrap().expect("SigUSD indexed");
    assert_eq!(row.name, "SigUSD");
    assert_eq!(row.decimals, Some(2));
    assert_eq!(row.emission, SIGUSD_EMISSION);
    assert_eq!(row.mint_height, 453051);

    assert!(rd.token(&[0xAAu8; 32]).unwrap().is_none());
}

/// A limit-1 walk of `tokens_newest` must visit every token exactly once, newest mint gidx
/// first, and reach both fixture tokens.
#[test]
fn tokens_newest_walks_every_token_newest_first_with_a_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let (s, new_token, _) = store_with_two_tokens(dir.path());
    let rd = Reader::new(&s).unwrap();

    let mut seen: Vec<(Hash32, u64)> = Vec::new();
    let mut cursor = None;
    loop {
        let page = rd.tokens_newest(cursor, 1).unwrap();
        for (id, row) in &page.items {
            seen.push((*id, row.mint_gidx));
        }
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(seen.len(), 2, "SigUSD plus the synthetic mint");
    // The synthetic mint is newer, so it comes first.
    assert_eq!(seen[0].0, new_token);
    assert_eq!(seen[1].0, sigusd());
    assert!(seen[0].1 > seen[1].1, "newest mint gidx first");

    // A single unpaged read returns the same tokens, and sets no cursor.
    let all = rd.tokens_newest(None, 500).unwrap();
    assert_eq!(
        all.items.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![new_token, sigusd()]
    );
    assert!(all.next_cursor.is_none());

    // A full page sets a cursor even when nothing follows it; the next read is then empty.
    let exact = rd.tokens_newest(None, 2).unwrap();
    let tail = rd.tokens_newest(exact.next_cursor, 2).unwrap();
    assert!(tail.items.is_empty());
    assert!(rd.tokens_newest(None, 0).unwrap().items.is_empty());
}

#[test]
fn tokens_by_holders_starts_at_the_largest_holder_count() {
    let dir = tempfile::tempdir().unwrap();
    let (s, new_token, _) = store_with_two_tokens(dir.path());
    let rd = Reader::new(&s).unwrap();

    // SigUSD ends on two trees, the synthetic mint on one.
    let (items, next) = rd.tokens_by_holders(None, 500).unwrap();
    assert!(next.is_none());
    let counts: Vec<(Hash32, u64)> = items.iter().map(|(id, r)| (*id, r.holder_count)).collect();
    assert_eq!(counts, vec![(sigusd(), 2), (new_token, 1)]);

    // Limit-1 walk visits the same tokens, in the same order.
    let mut walked = Vec::new();
    let mut cursor = None;
    loop {
        let (page, next) = rd.tokens_by_holders(cursor, 1).unwrap();
        walked.extend(page.into_iter().map(|(id, _)| id));
        match next {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(walked, vec![sigusd(), new_token]);
    assert!(rd.tokens_by_holders(None, 0).unwrap().0.is_empty());
}

/// Splits the SigUSD supply across three distinct trees and reads the holders back: largest
/// amount first, with a cursor round trip that resumes exactly after the last item.
#[test]
fn token_holders_are_ordered_by_amount_descending_with_a_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let s = store_with_453051(dir.path());
    let id = sigusd();
    let b = fixture(453051);
    let mint_box = b.txs[1].outputs[0].clone();

    // Three outputs on three distinct trees taken from the fixture itself, holding 3/2/1 of
    // the supply (the rest burns, which the holder tables do not care about).
    let trees = distinct_tree_outputs(&b);

    let mut outputs = Vec::new();
    for (i, (proto, amount)) in trees.iter().zip([3u64, 2, 1]).enumerate() {
        let mut o = proto.clone();
        o.id = BoxId([0xE0 + i as u8; 32]);
        o.tx_id = TxId([0xDDu8; 32]);
        o.index = i as u16;
        o.value = 1_000_000;
        o.tokens = vec![(id, amount)];
        outputs.push(o);
    }
    let expected: Vec<(Hash32, u64)> = outputs
        .iter()
        .map(|o| (o.tree_hash.0, o.tokens[0].1))
        .collect();

    let mut b2 = fixture(453051);
    b2.header.height = 453052;
    b2.header.parent_id = b.header.id;
    b2.header.id = HeaderId([0xC9u8; 32]);
    b2.txs = vec![DecodedTx {
        id: TxId([0xDDu8; 32]),
        inputs: vec![mint_box.id],
        data_inputs: vec![],
        outputs,
        size: 100,
    }];
    s.apply_batch(&[b2], true).unwrap();

    let rd = Reader::new(&s).unwrap();
    let (holders, next) = rd.token_holders(&id, None, 10).unwrap();
    assert_eq!(holders, expected, "descending by amount");
    assert!(next.is_none());

    // Cursor round trip: first page of one, then resume.
    let (first, cursor) = rd.token_holders(&id, None, 1).unwrap();
    assert_eq!(first, expected[..1]);
    assert_eq!(cursor, Some((expected[0].1, expected[0].0)));
    let (rest, _) = rd.token_holders(&id, cursor, 10).unwrap();
    assert_eq!(rest, expected[1..]);

    // An unknown token has no holders.
    let (none, _) = rd.token_holders(&[0xAAu8; 32], None, 10).unwrap();
    assert!(none.is_empty());
}

#[test]
fn token_boxes_returns_all_or_only_unspent_boxes_carrying_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let (s, new_token, synthetic) = store_with_two_tokens(dir.path());
    let id = sigusd();
    let mint_box_id = fixture(453051).txs[1].outputs[0].id.0;
    let rd = Reader::new(&s).unwrap();

    // The mint box was spent by the synthetic block; its two outputs are unspent.
    let all: HashSet<Hash32> = [mint_box_id, synthetic[0], synthetic[1]].into();
    let unspent: HashSet<Hash32> = [synthetic[0], synthetic[1]].into();

    let got: HashSet<Hash32> = rd
        .token_boxes(&id, false, None, 500, Dir::Asc)
        .unwrap()
        .items
        .into_iter()
        .map(|(bid, _)| bid)
        .collect();
    assert_eq!(got, all);

    let page = rd.token_boxes(&id, true, None, 500, Dir::Asc).unwrap();
    assert!(page.next_cursor.is_none());
    let got_unspent: HashSet<Hash32> = page.items.iter().map(|(bid, _)| *bid).collect();
    assert_eq!(got_unspent, unspent);
    for (bid, row) in &page.items {
        assert!(row.spent.is_none(), "unspent page returned a spent box");
        assert!(row.tokens.iter().any(|(t, _)| *t == id), "{bid:?}");
    }

    // Only the first synthetic output carries the second token.
    let other: Vec<Hash32> = rd
        .token_boxes(&new_token, false, None, 500, Dir::Asc)
        .unwrap()
        .items
        .into_iter()
        .map(|(bid, _)| bid)
        .collect();
    assert_eq!(other, vec![synthetic[0]]);

    // Descending returns the same set in reverse gidx order, and a limit-1 walk with the
    // cursor visits exactly the ascending order.
    let desc: Vec<u64> = rd
        .token_boxes(&id, false, None, 500, Dir::Desc)
        .unwrap()
        .items
        .iter()
        .map(|(_, r)| r.gidx)
        .collect();
    let mut sorted = desc.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(desc, sorted);
    assert_eq!(desc.len(), all.len());

    let mut walked = Vec::new();
    let mut cursor = None;
    loop {
        let page = rd.token_boxes(&id, false, cursor, 1, Dir::Asc).unwrap();
        walked.extend(page.items.into_iter().map(|(bid, _)| bid));
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(walked.len(), all.len());
    assert_eq!(walked.iter().copied().collect::<HashSet<_>>(), all);

    // An unknown token has no boxes.
    let none = rd
        .token_boxes(&[0xAAu8; 32], false, None, 500, Dir::Asc)
        .unwrap();
    assert!(none.items.is_empty());
}

#[test]
fn template_and_template_boxes_match_the_index_keys() {
    let (_dir, s, blocks) = store_with_1866000();

    // The template carried by the most fixture boxes (see `tests/apply.rs`).
    let mut counts: HashMap<Hash32, usize> = HashMap::new();
    for o in all_outputs(&blocks) {
        if let Ok(h) = xp_wire::template_hash_of(&o.tree_bytes) {
            *counts.entry(h).or_default() += 1;
        }
    }
    let tmpl = *counts
        .iter()
        .max_by_key(|(h, n)| (**n, **h))
        .expect("some template")
        .0;

    let rd = Reader::new(&s).unwrap();
    let row = rd.template(&tmpl).unwrap().expect("template row indexed");
    assert!(rd.template(&[0xAAu8; 32]).unwrap().is_none());

    let boxes = rd
        .template_boxes(&tmpl, false, None, 500, Dir::Asc)
        .unwrap()
        .items;
    assert_eq!(boxes.len() as u64, row.box_count);
    let unspent = rd
        .template_boxes(&tmpl, true, None, 500, Dir::Asc)
        .unwrap()
        .items;
    assert_eq!(unspent.len() as u64, row.unspent_count);
    assert!(unspent.len() < boxes.len(), "some fixture box is spent");
    for (_, r) in &unspent {
        assert!(r.spent.is_none());
    }
    // Every returned box really is on this template.
    for (_, r) in &boxes {
        let tree = rd.tree_row(&r.tree_hash).unwrap().expect("tree indexed");
        assert_eq!(tree.template_hash, tmpl);
    }

    // Limit-1 walk with a cursor visits exactly the same boxes.
    let mut walked = Vec::new();
    let mut cursor = None;
    loop {
        let page = rd
            .template_boxes(&tmpl, false, cursor, 1, Dir::Asc)
            .unwrap();
        walked.extend(page.items.into_iter().map(|(id, _)| id));
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    let expected: Vec<Hash32> = boxes.iter().map(|(id, _)| *id).collect();
    assert_eq!(walked, expected);
}

#[test]
fn boxes_by_register_finds_the_fixture_box_holding_the_value() {
    let (_dir, s, blocks) = store_with_1866000();
    let rd = Reader::new(&s).unwrap();

    let (probe, raw) = all_outputs(&blocks)
        .into_iter()
        .find_map(|o| {
            let after = o.registers_json.split_once("\"R4\":\"")?.1;
            let end = after.find('"')?;
            let raw = hex::decode(&after[..end]).ok()?;
            Some((o, raw))
        })
        .expect("some fixture output carries an R4");
    let value_hash = xp_wire::tree::blake2b256(&raw);

    let found = rd
        .boxes_by_register(4, &value_hash, None, 500, Dir::Asc)
        .unwrap();
    assert!(found.next_cursor.is_none());
    assert!(
        found.items.iter().any(|(id, _)| *id == probe.id.0),
        "register index did not return the probe box"
    );
    for (_, r) in &found.items {
        assert!(r.registers_json.contains(&hex::encode(&raw)));
    }

    // A register value no box holds, and a register the probe value lives in but under a
    // different index, both come back empty.
    let absent = rd
        .boxes_by_register(4, &xp_wire::tree::blake2b256(b"nope"), None, 500, Dir::Asc)
        .unwrap();
    assert!(absent.items.is_empty());
    let wrong_reg = rd
        .boxes_by_register(9, &value_hash, None, 500, Dir::Asc)
        .unwrap();
    assert!(wrong_reg.items.iter().all(|(id, _)| *id != probe.id.0));
}

#[test]
fn token_names_returns_only_the_ids_that_have_rows() {
    let dir = tempfile::tempdir().unwrap();
    let s = store_with_453051(dir.path());
    let rd = Reader::new(&s).unwrap();

    let unknown = [0xAAu8; 32];
    let got = rd.token_names(&[unknown, sigusd(), unknown]).unwrap();
    assert_eq!(got, vec![(sigusd(), "SigUSD".to_string(), Some(2u8))]);
    assert!(rd.token_names(&[]).unwrap().is_empty());
}
