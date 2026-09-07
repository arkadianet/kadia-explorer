//! Seeding of the chain-spec genesis boxes.
//!
//! Ergo mainnet's first boxes — the emission box, the foundation/treasury box and the
//! no-premine proof box — are created by the chain spec, not by any block, so they never
//! appear in block data. Height 1 spends one of them, and later heights spend the others.
//! Without them in the store, those spends look exactly like corruption (and, when
//! tolerated, silently produce wrong balances), so a full sync from height 1 writes them
//! first, from the node's `GET /utxo/genesis`.
//!
//! Because they belong to no transaction, each carries an all-zero `tx_id` and index 0, and
//! no `TXS`/`TX_BY_GIDX` row is written for them: a lookup of that all-zero tx id finds
//! nothing, and `boxes_of_tx` on it returns empty.

use redb::{Durability, ReadableTable};
use xp_wire::DecodedBox;

use crate::apply::{insert_output, upsert_tree};
use crate::extras::Extras;
use crate::keys::{k_rich, k_u64};
use crate::rows::BalanceRow;
use crate::tables::*;
use crate::tokens::Tokens;
use crate::{Store, StoreError};

impl Store {
    /// Whether [`Store::seed_genesis`] has already run on this store.
    pub fn genesis_seeded(&self) -> Result<bool, StoreError> {
        let txn = self.db.begin_read()?;
        let meta = txn.open_table(META)?;
        Ok(meta.get(META_GENESIS_SEEDED)?.is_some())
    }

    /// Writes the chain-spec genesis boxes into an empty store, in one transaction, exactly
    /// as `apply_batch` would write a block's outputs: boxes, gidx index, per-tree indexes,
    /// rent maturity, tree rows, balances and the rich list. `first_seen`/`last_seen` are 0,
    /// the height these boxes conceptually belong to.
    ///
    /// No undo row is written: genesis precedes every block, so it is never rolled back, and
    /// `rollback_to(0)` leaves these rows in place — which is what a store that then resumes
    /// from height 1 needs.
    ///
    /// Idempotent: a second call is `Ok(())` and changes nothing. Errors with
    /// [`StoreError::Corrupt`] if the store has already indexed a block, because the gidx
    /// numbering it would allocate would then collide with history already written.
    pub fn seed_genesis(&self, boxes: &[DecodedBox]) -> Result<(), StoreError> {
        if self.indexed_height()?.is_some() {
            return Err(StoreError::Corrupt("genesis seeding on a non-empty store"));
        }
        if self.genesis_seeded()? {
            return Ok(());
        }
        let mut txn = self.db.begin_write()?;
        txn.set_durability(Durability::Immediate);
        {
            let mut meta = txn.open_table(META)?;
            let mut next_box = meta
                .get(META_NEXT_BOX_GIDX)?
                .map(|v| crate::meta_u64(v.value()))
                .transpose()?
                .unwrap_or(0);

            let mut boxes_t = txn.open_table(BOXES)?;
            let mut box_by_gidx = txn.open_table(BOX_BY_GIDX)?;
            let mut ergo_trees = txn.open_table(ERGO_TREES)?;
            let mut tree_boxes = txn.open_table(TREE_BOXES)?;
            let mut tree_unspent = txn.open_table(TREE_UNSPENT)?;
            let mut rent_matures = txn.open_table(RENT_MATURES)?;
            let mut tree_balance = txn.open_table(TREE_BALANCE)?;
            let mut rich = txn.open_table(RICH)?;
            // Template and register indexes come from the same helper `apply_block` uses,
            // so the genesis boxes are indexed identically to any other box. The undo
            // bookkeeping it returns is discarded: genesis precedes every block.
            let mut extras = Extras::open(&txn)?;
            // Likewise for the token tables. A chain-spec box belongs to no transaction, so
            // it can mint and burn nothing; only its holdings are indexed. Mainnet's genesis
            // boxes carry no tokens at all, but the code path stays uniform.
            let mut tokens = Tokens::open(&txn)?;

            for b in boxes {
                let gidx = next_box;
                next_box += 1;
                let tree = b.tree_hash.0;

                upsert_tree(&mut ergo_trees, &tree, &b.tree_bytes, 0)?;
                // The per-box tables are written by the same helper `apply_block` uses for a
                // block's outputs, so the two paths cannot drift apart.
                insert_output(
                    &mut boxes_t,
                    &mut box_by_gidx,
                    &mut tree_boxes,
                    &mut tree_unspent,
                    &mut rent_matures,
                    gidx,
                    b,
                )?;
                extras.on_output(&ergo_trees, 0, gidx, b)?;
                tokens.on_output(gidx, b)?;

                // Balances and `RICH` are the one thing `insert_output` leaves to the caller:
                // apply.rs flushes them once per block from a cache, this seeds three boxes and
                // reads the row back per box (two genesis boxes could in principle share a
                // tree). The `BalanceRow` and `RICH` key contents must match apply.rs's.
                let mut bal = tree_balance
                    .get(tree.as_slice())?
                    .map(|v| BalanceRow::decode(v.value()))
                    .transpose()?
                    .unwrap_or(BalanceRow {
                        nano: 0,
                        tokens: vec![],
                        box_count: 0,
                        first_seen: 0,
                        last_seen: 0,
                        tx_count: 0,
                    });
                if bal.nano > 0 {
                    rich.remove(k_rich(bal.nano, &tree).as_slice())?;
                }
                bal.nano += b.value;
                bal.box_count += 1;
                for (id, amt) in &b.tokens {
                    match bal.tokens.iter_mut().find(|(i, _)| i == id) {
                        Some(e) => e.1 += amt,
                        None => bal.tokens.push((*id, *amt)),
                    }
                }
                if bal.nano > 0 {
                    rich.insert(k_rich(bal.nano, &tree).as_slice(), &[][..])?;
                }
                tree_balance.insert(tree.as_slice(), bal.encode().as_slice())?;
            }

            extras.finish()?;
            tokens.finish()?;

            // The emission box is the largest of the chain-spec boxes by five orders of
            // magnitude (93 M ERG against a treasury box of ~4 M and a proof box of 1 nanoERG),
            // so "largest value" identifies it unambiguously. Recorded here because it is the
            // only place the emission tree is ever seen as such: the API labels boxes on it
            // (`BoxDto::kind == "emission"`) by reading this key back.
            if let Some(emission) = boxes.iter().max_by_key(|b| b.value) {
                meta.insert(META_EMISSION_TREE_HASH, emission.tree_hash.0.as_slice())?;
            }
            meta.insert(META_NEXT_BOX_GIDX, k_u64(next_box).as_slice())?;
            meta.insert(META_GENESIS_SEEDED, &[1u8][..])?;
        }
        txn.commit()?;
        Ok(())
    }
}
