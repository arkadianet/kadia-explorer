//! Seeding of the chain-spec genesis boxes.
//!
//! Ergo mainnet's first boxes — the emission box, the foundation/treasury box and the
//! no-premine proof box — are created by the chain spec, not by any block, so they never
//! appear in block data. Height 1 spends one of them, and later heights spend the others.
//! Without them in the store, those spends look exactly like corruption (and, when
//! tolerated, silently produce wrong balances), so a full sync from height 1 writes them
//! first, from the node's `GET /utxo/genesis`.

use redb::{Durability, ReadableTable};
use xp_types::rent::maturity_height;
use xp_wire::DecodedBox;

use crate::apply::upsert_tree;
use crate::keys::{k_hash_gidx, k_rent, k_rich, k_u64};
use crate::rows::{BalanceRow, BoxRow};
use crate::tables::*;
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

            for b in boxes {
                let gidx = next_box;
                next_box += 1;
                let tree = b.tree_hash.0;

                upsert_tree(&mut ergo_trees, &tree, &b.tree_bytes, 0)?;

                let row = BoxRow {
                    gidx,
                    value: b.value,
                    tree_hash: tree,
                    creation_height: b.creation_height,
                    tx_id: b.tx_id.0,
                    index: b.index,
                    size: b.size,
                    tokens: b.tokens.clone(),
                    registers_json: b.registers_json.clone(),
                    spent: None,
                };
                boxes_t.insert(b.id.0.as_slice(), row.encode().as_slice())?;
                box_by_gidx.insert(k_u64(gidx).as_slice(), b.id.0.as_slice())?;
                tree_boxes.insert(k_hash_gidx(&tree, gidx).as_slice(), &[][..])?;
                tree_unspent.insert(k_hash_gidx(&tree, gidx).as_slice(), &[][..])?;
                rent_matures.insert(
                    k_rent(maturity_height(b.creation_height), gidx).as_slice(),
                    b.id.0.as_slice(),
                )?;

                // Two genesis boxes could in principle share a tree, so the balance is
                // read back per box rather than built once.
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

            meta.insert(META_NEXT_BOX_GIDX, crate::keys::k_u64(next_box).as_slice())?;
            meta.insert(META_GENESIS_SEEDED, &[1u8][..])?;
        }
        txn.commit()?;
        Ok(())
    }
}
