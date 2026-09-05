use redb::{Durability, ReadableTable, Table, WriteTransaction};
use std::collections::HashMap;
use xp_types::{rent::maturity_height, Gidx, Hash32};
use xp_wire::{tree_info, DecodedBlock};

use crate::keys::{k_hash_gidx, k_rent, k_rich, k_u32, k_u64};
use crate::rows::{BalanceRow, BoxRow, HeaderRow, TreeRow, TxRow, UndoRow};
use crate::tables::*;
use crate::{Store, StoreError};

struct Ctx<'t> {
    txn: &'t WriteTransaction,
    next_box: Gidx,
    next_tx: Gidx,
    undo: UndoRow,
    height: u32,
    timestamp: u64,
}

impl Store {
    /// Applies a contiguous, ascending run of blocks in a single write transaction.
    ///
    /// `blocks[0].header.height` must equal `indexed_height() + 1` (or `1` on an empty store);
    /// each block's `parent_id` must match the previous block's (or stored tip's) id, else
    /// [`StoreError::ParentMismatch`] is returned and nothing is committed.
    pub fn apply_batch(&self, blocks: &[DecodedBlock], durable: bool) -> Result<(), StoreError> {
        if blocks.is_empty() {
            return Ok(());
        }
        let mut txn = self.db.begin_write()?;
        txn.set_durability(if durable {
            Durability::Immediate
        } else {
            Durability::None
        });

        let (mut height, mut prev_id) = {
            let meta = txn.open_table(META)?;
            let h = meta
                .get(META_INDEXED_HEIGHT)?
                .map(|v| u32::from_be_bytes(v.value().try_into().unwrap()));
            match h {
                Some(h) => {
                    let headers = txn.open_table(HEADERS)?;
                    let row = HeaderRow::decode(
                        headers
                            .get(k_u32(h).as_slice())?
                            .ok_or(StoreError::Corrupt("missing tip header"))?
                            .value(),
                    )?;
                    (h, Some(row.id))
                }
                None => (0, None),
            }
        };
        let (mut next_box, mut next_tx) = {
            let meta = txn.open_table(META)?;
            let rd = |k: &[u8]| -> Result<Gidx, StoreError> {
                Ok(meta
                    .get(k)?
                    .map(|v| u64::from_be_bytes(v.value().try_into().unwrap()))
                    .unwrap_or(0))
            };
            (rd(META_NEXT_BOX_GIDX)?, rd(META_NEXT_TX_GIDX)?)
        };

        for b in blocks {
            if b.header.height != height + 1 {
                return Err(StoreError::ParentMismatch {
                    height: b.header.height,
                    have: format!("tip height {height}"),
                    want: format!("height {}", height + 1),
                });
            }
            if let Some(p) = prev_id {
                if p != b.header.parent_id.0 {
                    return Err(StoreError::ParentMismatch {
                        height: b.header.height,
                        have: hex::encode(p),
                        want: hex::encode(b.header.parent_id.0),
                    });
                }
            }
            let mut ctx = Ctx {
                txn: &txn,
                next_box,
                next_tx,
                height: b.header.height,
                timestamp: b.header.timestamp,
                undo: UndoRow {
                    created_boxes: vec![],
                    spent_boxes: vec![],
                    tx_ids: vec![],
                    prev_balances: vec![],
                    prev_next_box_gidx: next_box,
                    prev_next_tx_gidx: next_tx,
                    new_trees: vec![],
                },
            };
            apply_block(&mut ctx, b)?;
            next_box = ctx.next_box;
            next_tx = ctx.next_tx;

            txn.open_table(UNDO)?.insert(
                k_u32(b.header.height).as_slice(),
                ctx.undo.encode().as_slice(),
            )?;
            if b.header.height > crate::ROLLBACK_WINDOW {
                txn.open_table(UNDO)?
                    .remove(k_u32(b.header.height - crate::ROLLBACK_WINDOW - 1).as_slice())?;
            }
            height = b.header.height;
            prev_id = Some(b.header.id.0);
        }

        {
            let mut meta = txn.open_table(META)?;
            meta.insert(META_INDEXED_HEIGHT, k_u32(height).as_slice())?;
            meta.insert(META_NEXT_BOX_GIDX, k_u64(next_box).as_slice())?;
            meta.insert(META_NEXT_TX_GIDX, k_u64(next_tx).as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }
}

fn apply_block(ctx: &mut Ctx, b: &DecodedBlock) -> Result<(), StoreError> {
    let mut boxes = ctx.txn.open_table(BOXES)?;
    let mut box_by_gidx = ctx.txn.open_table(BOX_BY_GIDX)?;
    let mut ergo_trees = ctx.txn.open_table(ERGO_TREES)?;
    let mut tree_boxes = ctx.txn.open_table(TREE_BOXES)?;
    let mut tree_unspent = ctx.txn.open_table(TREE_UNSPENT)?;
    let mut tree_txs = ctx.txn.open_table(TREE_TXS)?;
    let mut rent_matures = ctx.txn.open_table(RENT_MATURES)?;
    let mut txs_table = ctx.txn.open_table(TXS)?;
    let mut tx_by_gidx = ctx.txn.open_table(TX_BY_GIDX)?;
    let mut tree_balance = ctx.txn.open_table(TREE_BALANCE)?;

    let mut fees = 0u64;
    let mut touched_balances: HashMap<Hash32, BalanceRow> = HashMap::new();

    for (ti, tx) in b.txs.iter().enumerate() {
        let tx_gidx = ctx.next_tx;
        ctx.next_tx += 1;
        let mut value_in = 0u64;
        let mut trees_in_tx: Vec<Hash32> = vec![];

        for inp in &tx.inputs {
            let existing = boxes
                .get(inp.0.as_slice())?
                .map(|v| BoxRow::decode(v.value()))
                .transpose()?;
            // A missing input means this box predates whatever height the store started
            // tracking from (block 1's inputs are the chain-spec genesis boxes; a store
            // seeded or bootstrapped at a later height has the same gap for its first
            // applied block). Treat it as external: skip, contributing nothing to balances,
            // rather than treating an out-of-view box as corruption.
            let mut row = match existing {
                Some(r) => r,
                None => continue,
            };
            if row.spent.is_some() {
                return Err(StoreError::Corrupt("double spend in block"));
            }
            row.spent = Some((tx.id.0, ctx.height));
            value_in += row.value;
            boxes.insert(inp.0.as_slice(), row.encode().as_slice())?;
            tree_unspent.remove(k_hash_gidx(&row.tree_hash, row.gidx).as_slice())?;
            rent_matures
                .remove(k_rent(maturity_height(row.creation_height), row.gidx).as_slice())?;

            let bal = load_balance(&tree_balance, &mut touched_balances, &row.tree_hash)?;
            bal.nano = bal.nano.saturating_sub(row.value);
            bal.box_count = bal.box_count.saturating_sub(1);
            sub_tokens(&mut bal.tokens, &row.tokens);
            bal.last_seen = ctx.height;

            trees_in_tx.push(row.tree_hash);
            ctx.undo.spent_boxes.push(inp.0);
        }

        let first_out_gidx = ctx.next_box;
        let mut value_out = 0u64;
        for o in &tx.outputs {
            let gidx = ctx.next_box;
            ctx.next_box += 1;
            value_out += o.value;

            if ergo_trees.get(o.tree_hash.0.as_slice())?.is_none() {
                let info = tree_info(&o.tree_bytes).unwrap_or_else(|_| xp_wire::TreeInfo {
                    template_hash: [0; 32],
                    address: hex::encode(&o.tree_bytes),
                    kind: xp_wire::TreeKind::Other,
                });
                ergo_trees.insert(
                    o.tree_hash.0.as_slice(),
                    TreeRow {
                        tree_bytes: o.tree_bytes.clone(),
                        template_hash: info.template_hash,
                        address: info.address,
                        kind: info.kind as u8,
                    }
                    .encode()
                    .as_slice(),
                )?;
                ctx.undo.new_trees.push(o.tree_hash.0);
            }

            let row = BoxRow {
                gidx,
                value: o.value,
                tree_hash: o.tree_hash.0,
                creation_height: o.creation_height,
                tx_id: o.tx_id.0,
                index: o.index,
                size: o.size,
                tokens: o.tokens.clone(),
                registers_json: o.registers_json.clone(),
                spent: None,
            };
            boxes.insert(o.id.0.as_slice(), row.encode().as_slice())?;
            box_by_gidx.insert(k_u64(gidx).as_slice(), o.id.0.as_slice())?;
            tree_boxes.insert(k_hash_gidx(&o.tree_hash.0, gidx).as_slice(), &[][..])?;
            tree_unspent.insert(k_hash_gidx(&o.tree_hash.0, gidx).as_slice(), &[][..])?;
            rent_matures.insert(
                k_rent(maturity_height(o.creation_height), gidx).as_slice(),
                o.id.0.as_slice(),
            )?;

            let bal = load_balance(&tree_balance, &mut touched_balances, &o.tree_hash.0)?;
            bal.nano += o.value;
            bal.box_count += 1;
            add_tokens(&mut bal.tokens, &o.tokens);
            if bal.first_seen == 0 {
                bal.first_seen = ctx.height;
            }
            bal.last_seen = ctx.height;

            trees_in_tx.push(o.tree_hash.0);
            ctx.undo.created_boxes.push(o.id.0);
        }

        // Index 0 is the block's coinbase-like emission tx: it has no fee semantics.
        let fee = if ti == 0 {
            0
        } else {
            value_in.saturating_sub(value_out)
        };
        fees += fee;

        trees_in_tx.sort();
        trees_in_tx.dedup();
        for t in trees_in_tx {
            tree_txs.insert(k_hash_gidx(&t, tx_gidx).as_slice(), &[][..])?;
        }

        let txrow = TxRow {
            height: ctx.height,
            index: ti as u16,
            gidx: tx_gidx,
            first_out_gidx,
            timestamp: ctx.timestamp,
            size: tx.size,
            fee,
            inputs: tx.inputs.iter().map(|i| i.0).collect(),
            data_inputs: tx.data_inputs.iter().map(|i| i.0).collect(),
            output_count: tx.outputs.len() as u16,
        };
        txs_table.insert(tx.id.0.as_slice(), txrow.encode().as_slice())?;
        tx_by_gidx.insert(k_u64(tx_gidx).as_slice(), tx.id.0.as_slice())?;
        ctx.undo.tx_ids.push(tx.id.0);
    }

    // Balances and the value-ordered "rich" index are flushed once per block.
    let mut rich = ctx.txn.open_table(RICH)?;
    for (tree, bal) in touched_balances {
        let prev = tree_balance
            .get(tree.as_slice())?
            .map(|v| BalanceRow::decode(v.value()))
            .transpose()?;
        if let Some(p) = &prev {
            rich.remove(k_rich(p.nano, &tree).as_slice())?;
        }
        ctx.undo.prev_balances.push((tree, prev));
        if bal.nano > 0 {
            rich.insert(k_rich(bal.nano, &tree).as_slice(), &[][..])?;
        }
        tree_balance.insert(tree.as_slice(), bal.encode().as_slice())?;
    }

    let reward = b
        .txs
        .first()
        .map(|t| t.outputs.iter().map(|o| o.value).sum::<u64>())
        .unwrap_or(0);
    let hrow = HeaderRow {
        id: b.header.id.0,
        parent_id: b.header.parent_id.0,
        timestamp: b.header.timestamp,
        difficulty: b.header.difficulty,
        miner_pk: b.header.miner_pk,
        tx_count: b.txs.len() as u32,
        size: b.size,
        fees,
        reward,
        version: b.header.version,
        raw_json: b.header.raw_json.clone(),
    };
    ctx.txn
        .open_table(HEADERS)?
        .insert(k_u32(ctx.height).as_slice(), hrow.encode().as_slice())?;
    ctx.txn
        .open_table(HEADER_BY_ID)?
        .insert(b.header.id.0.as_slice(), k_u32(ctx.height).as_slice())?;

    Ok(())
}

fn load_balance<'a>(
    tree_balance: &Table<'_, &'static [u8], &'static [u8]>,
    cache: &'a mut HashMap<Hash32, BalanceRow>,
    tree: &Hash32,
) -> Result<&'a mut BalanceRow, StoreError> {
    if !cache.contains_key(tree) {
        let row = tree_balance
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
        cache.insert(*tree, row);
    }
    Ok(cache.get_mut(tree).unwrap())
}

fn add_tokens(bal: &mut Vec<(Hash32, u64)>, add: &[(Hash32, u64)]) {
    for (id, amt) in add {
        match bal.iter_mut().find(|(i, _)| i == id) {
            Some(e) => e.1 += amt,
            None => bal.push((*id, *amt)),
        }
    }
}

fn sub_tokens(bal: &mut Vec<(Hash32, u64)>, sub: &[(Hash32, u64)]) {
    for (id, amt) in sub {
        if let Some(pos) = bal.iter().position(|(i, _)| i == id) {
            bal[pos].1 = bal[pos].1.saturating_sub(*amt);
            if bal[pos].1 == 0 {
                bal.swap_remove(pos);
            }
        }
    }
}
