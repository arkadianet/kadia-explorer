//! Apply-side indexing of tokens: mints, transfers, burns and per-address holdings.
//!
//! Seven schema-v2 tables describe a token's life:
//!
//! * `TOKENS` holds one [`TokenRow`] per token, created by the transaction that mints it and
//!   thereafter only updated (`burned`, `holder_count`, `box_count`).
//! * `TOKENS_BY_GIDX` (mint box gidx → id) gives a newest-first listing, `TOKENS_BY_HOLDERS`
//!   (holder count, id) a most-held-first one.
//! * `TOKEN_BOXES` / `TOKEN_UNSPENT` are the per-token analogues of `TREE_BOXES` /
//!   `TREE_UNSPENT`: every box that ever carried the token, and the still-unspent subset.
//! * `TOKEN_HOLDER_AMT` ((token, tree) → amount) is the authoritative per-holder balance;
//!   `TOKEN_HOLDERS` ((token, amount, tree) → ()) is the same data re-keyed for ordered
//!   listings. A holder is dropped from *both* the moment its amount reaches zero, so a tree
//!   that re-acquires the token is byte-identical to one that never held it — which is what
//!   makes `apply` + `rollback` an identity.
//!
//! Two consensus rules drive the whole module (see the plan's Global Constraints):
//!
//! * **Mint**: a token whose id equals the id of the transaction's FIRST input box is minted
//!   by that transaction; the minted amount is its total across the outputs, and its EIP-4
//!   metadata is read from the first output carrying it. This needs only the input's *id*,
//!   so it works even on a partial store that cannot resolve the input box itself.
//! * **Burn**: per transaction and token, `burned += max(0, in − out)`. `in` comes from the
//!   resolved input `BoxRow`s, so inputs a partial store cannot resolve contribute nothing
//!   (they are simply invisible to the index, exactly as their outputs' creation was).
//!
//! Like [`crate::extras`], [`Tokens`] owns its tables for the duration of one block (or of
//! genesis seeding) and caches per-block state — token rows and holder amounts — so each is
//! read and written at most once per block, and each appears in the undo row exactly once.

use redb::{ReadableTable, Table, WriteTransaction};
use std::collections::HashMap;
use xp_types::{Gidx, Hash32};
use xp_wire::registers::{minted_token_of, parse_eip4};
use xp_wire::{DecodedBox, DecodedTx};

use crate::keys::{k_by_count, k_hash_gidx, k_token_holder, k_token_tree, k_u64};
use crate::rows::{BoxRow, TokenRow};
use crate::tables::*;
use crate::StoreError;

/// Shorthand for this crate's uniform `&[u8] -> &[u8]` table shape.
type Tb<'txn> = Table<'txn, &'static [u8], &'static [u8]>;

/// What [`Tokens`] contributes to a block's `UndoRow`. `apply_block` moves these into the row
/// it writes; `Store::seed_genesis` discards them (genesis precedes every block).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct TokensUndo {
    pub new_tokens: Vec<Hash32>,
    pub prev_tokens: Vec<(Hash32, TokenRow)>,
    pub prev_holder_amts: Vec<(Hash32, Hash32, Option<u64>)>,
}

/// How much of one token a transaction consumed and produced. `in_` is summed over the
/// *resolved* input boxes only.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TokenDelta {
    pub in_: u64,
    pub out: u64,
}

/// Per-token `(in, out)` totals for one transaction — the input to the burn rule.
///
/// Amounts saturate rather than wrap: consensus caps a token's supply well below `u64::MAX`,
/// but this must not panic on a debug build fed a hand-built block.
pub(crate) fn token_deltas(tx: &DecodedTx, input_boxes: &[BoxRow]) -> HashMap<Hash32, TokenDelta> {
    let mut m: HashMap<Hash32, TokenDelta> = HashMap::new();
    for b in input_boxes {
        for (id, amt) in &b.tokens {
            let e = m.entry(*id).or_default();
            e.in_ = e.in_.saturating_add(*amt);
        }
    }
    for o in &tx.outputs {
        for (id, amt) in &o.tokens {
            let e = m.entry(*id).or_default();
            e.out = e.out.saturating_add(*amt);
        }
    }
    m
}

/// A holder's amount as this block found it (`prev`, `None` = not a holder) and as it stands
/// now (`cur`). Flushed once, in [`Tokens::finish`].
#[derive(Debug, Clone, Copy)]
struct Holder {
    cur: u64,
    prev: Option<u64>,
}

pub(crate) struct Tokens<'txn> {
    tokens: Tb<'txn>,
    tokens_by_gidx: Tb<'txn>,
    tokens_by_holders: Tb<'txn>,
    token_boxes: Tb<'txn>,
    token_unspent: Tb<'txn>,
    token_holders: Tb<'txn>,
    token_holder_amt: Tb<'txn>,
    /// token id → (row being built, row as stored before this block began).
    rows: HashMap<Hash32, (TokenRow, Option<TokenRow>)>,
    /// (token, tree) → working holder amount.
    holders: HashMap<(Hash32, Hash32), Holder>,
    undo: TokensUndo,
}

impl<'txn> Tokens<'txn> {
    /// Opens the seven tables this module owns. They must not be opened by the caller as
    /// well: redb allows a table to be open only once per write transaction.
    pub(crate) fn open(txn: &'txn WriteTransaction) -> Result<Self, StoreError> {
        Ok(Tokens {
            tokens: txn.open_table(TOKENS)?,
            tokens_by_gidx: txn.open_table(TOKENS_BY_GIDX)?,
            tokens_by_holders: txn.open_table(TOKENS_BY_HOLDERS)?,
            token_boxes: txn.open_table(TOKEN_BOXES)?,
            token_unspent: txn.open_table(TOKEN_UNSPENT)?,
            token_holders: txn.open_table(TOKEN_HOLDERS)?,
            token_holder_amt: txn.open_table(TOKEN_HOLDER_AMT)?,
            rows: HashMap::new(),
            holders: HashMap::new(),
            undo: TokensUndo::default(),
        })
    }

    /// The cached working [`TokenRow`] for `token`, or `None` if the store holds no row for it.
    ///
    /// A missing row is normal, not corruption: on a partial store the mint may predate the
    /// seed point, so transfers and burns of such a token are still indexed in the holder and
    /// box tables — there is simply no row whose counters could be updated.
    fn row_mut(&mut self, token: &Hash32) -> Result<Option<&mut TokenRow>, StoreError> {
        if !self.rows.contains_key(token) {
            let Some(prev) = self
                .tokens
                .get(token.as_slice())?
                .map(|v| TokenRow::decode(v.value()))
                .transpose()?
            else {
                return Ok(None);
            };
            self.rows.insert(*token, (prev.clone(), Some(prev)));
        }
        Ok(Some(
            &mut self.rows.get_mut(token).expect("just inserted").0,
        ))
    }

    /// The cached working holder amount for `(token, tree)`, loaded from `TOKEN_HOLDER_AMT`
    /// (and remembering the loaded value as the undo baseline) on first touch this block.
    fn holder_mut(&mut self, token: Hash32, tree: Hash32) -> Result<&mut Holder, StoreError> {
        if !self.holders.contains_key(&(token, tree)) {
            let prev = self
                .token_holder_amt
                .get(k_token_tree(&token, &tree).as_slice())?
                .map(|v| crate::meta_u64(v.value()))
                .transpose()?;
            self.holders.insert(
                (token, tree),
                Holder {
                    cur: prev.unwrap_or(0),
                    prev,
                },
            );
        }
        Ok(self.holders.get_mut(&(token, tree)).expect("just inserted"))
    }

    /// Indexes one newly created box's tokens: box membership, the holder's amount, and the
    /// token's `box_count`. Shared by [`Tokens::apply_tx`] and `Store::seed_genesis` (whose
    /// chain-spec boxes belong to no transaction, so they can mint and burn nothing).
    pub(crate) fn on_output(&mut self, gidx: Gidx, o: &DecodedBox) -> Result<(), StoreError> {
        // A box's asset list may name the same token more than once (its amounts then simply
        // add up, and both the key insert and the holder credit handle that correctly), but
        // `box_count` counts *boxes*, so it must only move on a token's first appearance.
        let mut counted: Vec<Hash32> = Vec::with_capacity(o.tokens.len());
        for (id, amt) in &o.tokens {
            let k = k_hash_gidx(id, gidx);
            self.token_boxes.insert(k.as_slice(), &[][..])?;
            self.token_unspent.insert(k.as_slice(), &[][..])?;
            let h = self.holder_mut(*id, o.tree_hash.0)?;
            h.cur = h.cur.saturating_add(*amt);
            let first = !counted.contains(id);
            if first {
                counted.push(*id);
            }
            if let Some(row) = self.row_mut(id)? {
                if first {
                    row.box_count += 1;
                }
            }
        }
        Ok(())
    }

    /// Drops a spent box from every one of its tokens' unspent sets and debits its holder.
    ///
    /// Underflow policy matches `apply::debit_balance`: on a fully-synced store it means our
    /// own bookkeeping is wrong, so it errors; a partial store can legitimately see it (the
    /// output that credited this holder predates the seed point).
    fn on_spend(&mut self, b: &BoxRow, partial: bool) -> Result<(), StoreError> {
        for (id, amt) in &b.tokens {
            self.token_unspent
                .remove(k_hash_gidx(id, b.gidx).as_slice())?;
            let h = self.holder_mut(*id, b.tree_hash)?;
            h.cur = if partial {
                h.cur.saturating_sub(*amt)
            } else {
                h.cur
                    .checked_sub(*amt)
                    .ok_or(StoreError::Corrupt("token holder underflow"))?
            };
        }
        Ok(())
    }

    /// Creates the [`TokenRow`] for the token this transaction mints, if it mints one.
    ///
    /// Only the first input's *id* is needed, so this works on a partial store that cannot
    /// resolve the box itself. A token id is a box id, so it can never be minted twice; the
    /// "already known" guard is defensive only.
    fn on_mint(
        &mut self,
        tx: &DecodedTx,
        height: u32,
        out_gidx_start: Gidx,
    ) -> Result<(), StoreError> {
        let Some(first_input) = tx.inputs.first() else {
            return Ok(());
        };
        let Some((id, emission)) = minted_token_of(&first_input.0, &tx.outputs) else {
            return Ok(());
        };
        if self.rows.contains_key(&id) || self.tokens.get(id.as_slice())?.is_some() {
            return Ok(());
        }
        // EIP-4 metadata is defined to live on the FIRST output carrying the token.
        let Some((i, o)) = tx
            .outputs
            .iter()
            .enumerate()
            .find(|(_, o)| o.tokens.iter().any(|(t, _)| *t == id))
        else {
            return Ok(());
        };
        let meta = parse_eip4(&o.registers_json);
        let mint_gidx = out_gidx_start + i as u64;
        self.tokens_by_gidx
            .insert(k_u64(mint_gidx).as_slice(), id.as_slice())?;
        self.rows.insert(
            id,
            (
                TokenRow {
                    mint_tx: tx.id.0,
                    mint_box: o.id.0,
                    mint_height: height,
                    mint_gidx,
                    name: meta.name,
                    description: meta.description,
                    decimals: meta.decimals,
                    token_type: meta.token_type,
                    emission,
                    burned: 0,
                    holder_count: 0,
                    box_count: 0,
                },
                None,
            ),
        );
        Ok(())
    }

    /// Indexes one transaction's tokens: its mint (if any), its inputs' and outputs' effect on
    /// the box and holder tables, and finally its burns.
    ///
    /// `input_boxes` are the transaction's *resolved* inputs, in input order; `out_gidx_start`
    /// is the gidx allocated to `tx.outputs[0]`. The mint runs first so the freshly created
    /// row is the one the output loop then counts boxes and holders against.
    pub(crate) fn apply_tx(
        &mut self,
        tx: &DecodedTx,
        input_boxes: &[BoxRow],
        out_gidx_start: Gidx,
        height: u32,
        partial: bool,
    ) -> Result<(), StoreError> {
        self.on_mint(tx, height, out_gidx_start)?;
        for b in input_boxes {
            self.on_spend(b, partial)?;
        }
        for (i, o) in tx.outputs.iter().enumerate() {
            self.on_output(out_gidx_start + i as u64, o)?;
        }
        let mut deltas: Vec<(Hash32, TokenDelta)> = token_deltas(tx, input_boxes)
            .into_iter()
            .filter(|(_, d)| d.in_ > d.out)
            .collect();
        // Sorted so the undo row's `prev_tokens` order cannot depend on hash iteration order.
        deltas.sort_unstable_by_key(|(id, _)| *id);
        for (id, d) in deltas {
            let burned = d.in_ - d.out;
            if let Some(row) = self.row_mut(&id)? {
                row.burned = row.burned.saturating_add(burned);
            }
        }
        Ok(())
    }

    /// Flushes the cached holder amounts and token rows, and yields the undo bookkeeping.
    /// Consumes `self` so the seven tables are closed before the caller opens anything else.
    ///
    /// Holders are flushed first because they determine each token's `holder_count`, which in
    /// turn keys `TOKENS_BY_HOLDERS`. Both caches are drained from a `HashMap`, so both are
    /// sorted before being written or handed over: the encoded `UndoRow` is a stored value,
    /// and a store whose bytes depend on hash iteration order cannot be compared across runs.
    pub(crate) fn finish(mut self) -> Result<TokensUndo, StoreError> {
        let mut holders: Vec<((Hash32, Hash32), Holder)> =
            std::mem::take(&mut self.holders).into_iter().collect();
        holders.sort_unstable_by_key(|(k, _)| *k);

        // Net change to each token's holder count: a holder is one that holds a non-zero
        // amount, so only 0 → >0 and >0 → 0 transitions move the count.
        let mut count_delta: HashMap<Hash32, i64> = HashMap::new();
        for ((token, tree), h) in holders {
            let prev = h.prev.unwrap_or(0);
            if h.prev != Some(h.cur) {
                if prev > 0 {
                    self.token_holders
                        .remove(k_token_holder(&token, prev, &tree).as_slice())?;
                }
                if h.cur > 0 {
                    self.token_holders
                        .insert(k_token_holder(&token, h.cur, &tree).as_slice(), &[][..])?;
                    self.token_holder_amt.insert(
                        k_token_tree(&token, &tree).as_slice(),
                        k_u64(h.cur).as_slice(),
                    )?;
                } else {
                    self.token_holder_amt
                        .remove(k_token_tree(&token, &tree).as_slice())?;
                }
            }
            match (prev, h.cur) {
                (0, c) if c > 0 => *count_delta.entry(token).or_default() += 1,
                (p, 0) if p > 0 => *count_delta.entry(token).or_default() -= 1,
                _ => {}
            }
            self.undo.prev_holder_amts.push((token, tree, h.prev));
        }

        let mut deltas: Vec<(Hash32, i64)> = count_delta.into_iter().collect();
        deltas.sort_unstable_by_key(|(id, _)| *id);
        for (token, d) in deltas {
            if let Some(row) = self.row_mut(&token)? {
                // Saturating rather than checked: on a partial store a holder can drop to
                // zero without this store ever having seen it acquire the token, so a
                // "negative" holder count is a legitimate consequence of the missing history
                // rather than corruption (the same policy as `apply::debit_balance`).
                row.holder_count = if d >= 0 {
                    row.holder_count.saturating_add(d as u64)
                } else {
                    row.holder_count.saturating_sub(d.unsigned_abs())
                };
            }
        }

        let mut rows: Vec<(Hash32, (TokenRow, Option<TokenRow>))> =
            std::mem::take(&mut self.rows).into_iter().collect();
        rows.sort_unstable_by_key(|(id, _)| *id);
        for (id, (row, prev)) in rows {
            match prev {
                Some(p) => {
                    if p.holder_count != row.holder_count {
                        self.tokens_by_holders
                            .remove(k_by_count(p.holder_count, &id).as_slice())?;
                        self.tokens_by_holders
                            .insert(k_by_count(row.holder_count, &id).as_slice(), &[][..])?;
                    }
                    self.undo.prev_tokens.push((id, p));
                }
                None => {
                    self.tokens_by_holders
                        .insert(k_by_count(row.holder_count, &id).as_slice(), &[][..])?;
                    self.undo.new_tokens.push(id);
                }
            }
            self.tokens.insert(id.as_slice(), row.encode().as_slice())?;
        }
        Ok(self.undo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xp_types::{BoxId, TxId};

    fn dbox(tokens: Vec<(Hash32, u64)>) -> DecodedBox {
        DecodedBox {
            id: BoxId([0; 32]),
            value: 1,
            tree_bytes: vec![],
            tree_hash: xp_types::TreeHash([0; 32]),
            creation_height: 0,
            tx_id: TxId([0; 32]),
            index: 0,
            tokens,
            registers_json: "{}".into(),
            id_verified: true,
            size: 0,
        }
    }

    fn brow(tokens: Vec<(Hash32, u64)>) -> BoxRow {
        BoxRow {
            gidx: 0,
            value: 1,
            tree_hash: [0; 32],
            creation_height: 0,
            tx_id: [0; 32],
            index: 0,
            size: 0,
            tokens,
            registers_json: "{}".into(),
            spent: None,
        }
    }

    /// The burn rule's raw material: inputs sum into `in_`, outputs into `out`, per token,
    /// with tokens appearing on only one side still present (with a zero on the other).
    #[test]
    fn token_deltas_sum_both_sides_per_token() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let c = [3u8; 32];
        let tx = DecodedTx {
            id: TxId([0; 32]),
            inputs: vec![],
            data_inputs: vec![],
            outputs: vec![dbox(vec![(a, 3), (c, 7)]), dbox(vec![(a, 4)])],
            size: 0,
        };
        let inputs = [brow(vec![(a, 10), (b, 5)]), brow(vec![(a, 1)])];
        let d = token_deltas(&tx, &inputs);
        assert_eq!(d[&a], TokenDelta { in_: 11, out: 7 });
        assert_eq!(d[&b], TokenDelta { in_: 5, out: 0 });
        assert_eq!(d[&c], TokenDelta { in_: 0, out: 7 });
    }

    /// With no resolved inputs (a partial store's view of an old box) nothing is consumed, so
    /// nothing can burn — outputs alone are still counted.
    #[test]
    fn token_deltas_without_resolved_inputs_consume_nothing() {
        let a = [9u8; 32];
        let tx = DecodedTx {
            id: TxId([0; 32]),
            inputs: vec![BoxId([4; 32])],
            data_inputs: vec![],
            outputs: vec![dbox(vec![(a, 2)])],
            size: 0,
        };
        let d = token_deltas(&tx, &[]);
        assert_eq!(d[&a], TokenDelta { in_: 0, out: 2 });
    }
}
