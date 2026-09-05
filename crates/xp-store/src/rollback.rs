//! Reverses previously applied blocks using the [`rows::UndoRow`]s written by
//! [`crate::apply`], restoring the store to exactly the state it held before those blocks
//! were applied. See [`Store::rollback_to`].

use redb::ReadableTable;
use xp_types::rent::maturity_height;

use crate::keys::{k_hash_gidx, k_rent, k_rich, k_u32, k_u64};
use crate::rows::{BalanceRow, BoxRow, HeaderRow, TxRow, UndoRow};
use crate::tables::*;
use crate::{Store, StoreError, ROLLBACK_WINDOW};

impl Store {
    /// Rolls the store back to `target` (inclusive), undoing every block above it in
    /// descending height order using each height's stored [`rows::UndoRow`]. A no-op if the
    /// store's tip is already at or below `target`. Refuses (without touching the store) if
    /// `target` is more than [`ROLLBACK_WINDOW`] blocks behind the tip, since undo rows that
    /// far back have already been pruned by [`crate::apply`]. The whole rollback — every
    /// undone height — runs in one write transaction: any error aborts the entire operation,
    /// leaving the store exactly as it was before the call.
    pub fn rollback_to(&self, target: u32) -> Result<(), StoreError> {
        let tip = self
            .indexed_height()?
            .ok_or(StoreError::Corrupt("rollback on empty store"))?;
        if tip <= target {
            return Ok(());
        }
        if tip - target > ROLLBACK_WINDOW {
            return Err(StoreError::ReindexRequired(tip - target));
        }

        let txn = self.db.begin_write()?;
        for h in (target + 1..=tip).rev() {
            let undo = {
                let u = txn.open_table(UNDO)?;
                // Bind the decoded row before the block ends so the `AccessGuard` borrowed
                // from `u` (and `u` itself) drop here, before the next `txn.open_table` call
                // below tries to open a different table for writing.
                let decoded = UndoRow::decode(
                    u.get(k_u32(h).as_slice())?
                        .ok_or(StoreError::ReindexRequired(tip - target))?
                        .value(),
                )?;
                decoded
            };

            // 1. Un-create boxes this block created (reverse of apply's output loop).
            for id in undo.created_boxes.iter().rev() {
                let row = {
                    let b = txn.open_table(BOXES)?;
                    let decoded = BoxRow::decode(
                        b.get(id.as_slice())?
                            .ok_or(StoreError::Corrupt("undo: created box missing"))?
                            .value(),
                    )?;
                    decoded
                };
                txn.open_table(BOXES)?.remove(id.as_slice())?;
                txn.open_table(BOX_BY_GIDX)?
                    .remove(k_u64(row.gidx).as_slice())?;
                txn.open_table(TREE_BOXES)?
                    .remove(k_hash_gidx(&row.tree_hash, row.gidx).as_slice())?;
                txn.open_table(TREE_UNSPENT)?
                    .remove(k_hash_gidx(&row.tree_hash, row.gidx).as_slice())?;
                txn.open_table(RENT_MATURES)?
                    .remove(k_rent(maturity_height(row.creation_height), row.gidx).as_slice())?;
            }

            // 2. Un-spend inputs this block spent. This MUST run after step 1 (un-create):
            // a box created and spent within the same block is in both `created_boxes` and
            // `spent_boxes`, step 1 already removed its BOXES row, and step 2 finds nothing
            // for it — that is the only reason `existing` can legitimately be `None` here.
            // (apply.rs's tolerated-missing-input case at height 1 / on a partial store
            // `continue`s before ever pushing onto `spent_boxes`, so that case never reaches
            // this loop at all.) The `if let Some` guard exists solely for the same-block
            // create-then-spend case.
            for id in &undo.spent_boxes {
                let existing = {
                    let b = txn.open_table(BOXES)?;
                    let decoded = b
                        .get(id.as_slice())?
                        .map(|v| BoxRow::decode(v.value()))
                        .transpose()?;
                    decoded
                };
                if let Some(mut row) = existing {
                    row.spent = None;
                    txn.open_table(BOXES)?
                        .insert(id.as_slice(), row.encode().as_slice())?;
                    txn.open_table(TREE_UNSPENT)?
                        .insert(k_hash_gidx(&row.tree_hash, row.gidx).as_slice(), &[][..])?;
                    txn.open_table(RENT_MATURES)?.insert(
                        k_rent(maturity_height(row.creation_height), row.gidx).as_slice(),
                        id.as_slice(),
                    )?;
                }
            }

            // 3. Remove this block's txs and every TREE_TXS key it inserted.
            for tid in &undo.tx_ids {
                let row = {
                    let t = txn.open_table(TXS)?;
                    let decoded = TxRow::decode(
                        t.get(tid.as_slice())?
                            .ok_or(StoreError::Corrupt("undo: tx missing"))?
                            .value(),
                    )?;
                    decoded
                };
                txn.open_table(TXS)?.remove(tid.as_slice())?;
                txn.open_table(TX_BY_GIDX)?
                    .remove(k_u64(row.gidx).as_slice())?;
            }
            {
                let mut tt = txn.open_table(TREE_TXS)?;
                for (tree, gidx) in &undo.tree_txs {
                    tt.remove(k_hash_gidx(tree, *gidx).as_slice())?;
                }
            }

            // 4. Restore balances (and the value-ordered RICH index) wholesale from the
            // undo row's snapshot — no arithmetic needed, unlike apply's debits/credits.
            for (tree, prev) in &undo.prev_balances {
                let mut tb = txn.open_table(TREE_BALANCE)?;
                let mut rich = txn.open_table(RICH)?;
                if let Some(cur) = tb
                    .get(tree.as_slice())?
                    .map(|v| BalanceRow::decode(v.value()))
                    .transpose()?
                {
                    rich.remove(k_rich(cur.nano, tree).as_slice())?;
                }
                match prev {
                    Some(p) => {
                        tb.insert(tree.as_slice(), p.encode().as_slice())?;
                        if p.nano > 0 {
                            rich.insert(k_rich(p.nano, tree).as_slice(), &[][..])?;
                        }
                    }
                    None => {
                        tb.remove(tree.as_slice())?;
                    }
                }
            }

            // 5. Drop ERGO_TREES entries first seen in this block.
            {
                let mut trees = txn.open_table(ERGO_TREES)?;
                for t in &undo.new_trees {
                    trees.remove(t.as_slice())?;
                }
            }

            // 6. Remove the header, its undo row, and roll counters/indexed_height back.
            let hid = {
                let hd = txn.open_table(HEADERS)?;
                let decoded = HeaderRow::decode(
                    hd.get(k_u32(h).as_slice())?
                        .ok_or(StoreError::Corrupt("undo: header missing"))?
                        .value(),
                )?;
                decoded.id
            };
            txn.open_table(HEADERS)?.remove(k_u32(h).as_slice())?;
            txn.open_table(HEADER_BY_ID)?.remove(hid.as_slice())?;
            txn.open_table(UNDO)?.remove(k_u32(h).as_slice())?;

            let mut meta = txn.open_table(META)?;
            meta.insert(
                META_NEXT_BOX_GIDX,
                k_u64(undo.prev_next_box_gidx).as_slice(),
            )?;
            meta.insert(META_NEXT_TX_GIDX, k_u64(undo.prev_next_tx_gidx).as_slice())?;
            if h == 1 {
                // Rolling back the first-ever block leaves an empty store. `indexed_height()`
                // treats an *absent* META_INDEXED_HEIGHT as "empty" (see `apply_batch`'s
                // `None` branch, which starts from height 0 with no tip header to look up);
                // writing `k_u32(0)` here instead would make the store look like it has a
                // real height-0 tip, which apply_batch would then try to load a HEADERS row
                // for and fail. So a rollback to height 0 must be indistinguishable from a
                // freshly opened store.
                meta.remove(META_INDEXED_HEIGHT)?;
            } else {
                meta.insert(META_INDEXED_HEIGHT, k_u32(h - 1).as_slice())?;
            }
        }
        txn.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Focused test for the `h == 1` branch of `rollback_to`: rolling back the first-ever
    /// block must leave the store indistinguishable from one that was never applied to
    /// (`indexed_height() == None`), not one sitting at a real height-0 tip. Hand-constructs
    /// a minimal height-1 store (header + undo row + META_INDEXED_HEIGHT) directly through
    /// `begin_write`, since no fixture/`apply_batch` call is needed to exercise this branch.
    #[test]
    fn rollback_to_zero_leaves_store_indistinguishable_from_fresh() {
        let dir = tempdir().unwrap();
        let s = Store::open(&dir.path().join("x.redb")).unwrap();

        let txn = s.db.begin_write().unwrap();
        {
            let hrow = HeaderRow {
                id: [1; 32],
                parent_id: [0; 32],
                timestamp: 0,
                difficulty: 0,
                miner_pk: [0; 33],
                tx_count: 0,
                size: 0,
                fees: 0,
                reward: 0,
                version: 0,
                raw_json: String::new(),
            };
            txn.open_table(HEADERS)
                .unwrap()
                .insert(k_u32(1).as_slice(), hrow.encode().as_slice())
                .unwrap();
            txn.open_table(HEADER_BY_ID)
                .unwrap()
                .insert([1u8; 32].as_slice(), k_u32(1).as_slice())
                .unwrap();
            let undo = UndoRow {
                created_boxes: vec![],
                spent_boxes: vec![],
                tx_ids: vec![],
                tree_txs: vec![],
                prev_balances: vec![],
                prev_next_box_gidx: 0,
                prev_next_tx_gidx: 0,
                new_trees: vec![],
            };
            txn.open_table(UNDO)
                .unwrap()
                .insert(k_u32(1).as_slice(), undo.encode().as_slice())
                .unwrap();
            txn.open_table(META)
                .unwrap()
                .insert(META_INDEXED_HEIGHT, k_u32(1).as_slice())
                .unwrap();
        }
        txn.commit().unwrap();

        assert_eq!(s.indexed_height().unwrap(), Some(1));
        s.rollback_to(0).unwrap();
        assert_eq!(s.indexed_height().unwrap(), None);
        // apply_batch on an empty slice is a no-op regardless of store state; also confirms
        // the store isn't left in some half-rolled-back state that errors on the next call.
        assert!(s.apply_batch(&[], true).is_ok());
    }
}
