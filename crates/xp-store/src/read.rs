//! `Reader`: a consistent point-in-time view over a [`Store`], backed by a single
//! `redb::ReadTransaction` held for the reader's lifetime so every method sees the same
//! snapshot regardless of concurrent writers.

use crate::keys::{k_prefix_gidx, k_rent, k_rich, k_u32, k_u64, prefix_range};
use crate::rows::{BalanceRow, BoxRow, HeaderRow, TreeRow, TxRow};
use crate::tables::*;
use crate::{Store, StoreError};
use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use redb::{ReadTransaction, ReadableTable};
use std::ops::Bound;
use xp_types::{Gidx, Hash32};

pub struct Reader {
    pub(crate) txn: ReadTransaction,
}

pub enum Dir {
    Asc,
    Desc,
}

pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Gidx>,
}

/// Decodes a table value expected to be exactly a 32-byte id, failing with
/// [`StoreError::Corrupt`] on any other width.
pub(crate) fn as_hash32(v: &[u8]) -> Result<Hash32, StoreError> {
    v.try_into()
        .map_err(|_| StoreError::Corrupt("bad hash32 value"))
}

impl Reader {
    pub fn new(store: &Store) -> Result<Reader, StoreError> {
        Ok(Reader {
            txn: store.begin_read()?,
        })
    }

    pub fn indexed_height(&self) -> Result<Option<u32>, StoreError> {
        let meta = self.txn.open_table(META)?;
        meta.get(META_INDEXED_HEIGHT)?
            .map(|v| crate::meta_u32(v.value()))
            .transpose()
    }

    /// Ergo tree hash of the chain-spec emission box, or `None` on a store that never
    /// seeded genesis. See [`crate::tables::META_EMISSION_TREE_HASH`].
    pub fn emission_tree_hash(&self) -> Result<Option<Hash32>, StoreError> {
        let meta = self.txn.open_table(META)?;
        meta.get(META_EMISSION_TREE_HASH)?
            .map(|v| as_hash32(v.value()))
            .transpose()
    }

    pub fn header_at(&self, height: u32) -> Result<Option<HeaderRow>, StoreError> {
        let table = self.txn.open_table(HEADERS)?;
        match table.get(k_u32(height).as_slice())? {
            Some(v) => Ok(Some(HeaderRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn height_of_header(&self, id: &Hash32) -> Result<Option<u32>, StoreError> {
        let table = self.txn.open_table(HEADER_BY_ID)?;
        match table.get(id.as_slice())? {
            Some(v) => Ok(Some(crate::meta_u32(v.value())?)),
            None => Ok(None),
        }
    }

    /// Headers strictly below `before_height` (or below the tip when `None`), newest first,
    /// capped at `limit`.
    pub fn headers_desc(
        &self,
        before_height: Option<u32>,
        limit: usize,
    ) -> Result<Vec<(u32, HeaderRow)>, StoreError> {
        let hi_exclusive = match before_height {
            Some(h) => h,
            None => match self.indexed_height()? {
                Some(tip) => tip + 1,
                None => return Ok(vec![]),
            },
        };
        if hi_exclusive == 0 {
            return Ok(vec![]);
        }
        let table = self.txn.open_table(HEADERS)?;
        let mut out = Vec::new();
        for item in table
            .range::<&[u8]>((
                Bound::Unbounded,
                Bound::Excluded(k_u32(hi_exclusive).as_slice()),
            ))?
            .rev()
        {
            if out.len() >= limit {
                break;
            }
            let (k, v) = item?;
            let height = crate::meta_u32(k.value())?;
            out.push((height, HeaderRow::decode(v.value())?));
        }
        Ok(out)
    }

    pub fn tx_by_id(&self, id: &Hash32) -> Result<Option<TxRow>, StoreError> {
        let table = self.txn.open_table(TXS)?;
        match table.get(id.as_slice())? {
            Some(v) => Ok(Some(TxRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    pub(crate) fn resolve_tx(
        &self,
        txs: &impl ReadableTable<&'static [u8], &'static [u8]>,
        tx_id: Hash32,
    ) -> Result<(Hash32, TxRow), StoreError> {
        let row = txs
            .get(tx_id.as_slice())?
            .map(|v| TxRow::decode(v.value()))
            .transpose()?
            .ok_or(StoreError::Corrupt("dangling gidx->tx_id entry"))?;
        Ok((tx_id, row))
    }

    pub fn txs_by_gidx(
        &self,
        cursor: Option<Gidx>,
        limit: usize,
        dir: Dir,
    ) -> Result<Page<(Hash32, TxRow)>, StoreError> {
        let by_gidx = self.txn.open_table(TX_BY_GIDX)?;
        let txs = self.txn.open_table(TXS)?;
        let mut items = Vec::new();
        let mut last_gidx = None;
        match dir {
            Dir::Asc => {
                let lo = k_u64(cursor.map(|c| c.saturating_add(1)).unwrap_or(0));
                for item in
                    by_gidx.range::<&[u8]>((Bound::Included(lo.as_slice()), Bound::Unbounded))?
                {
                    if items.len() >= limit {
                        break;
                    }
                    let (k, v) = item?;
                    let gidx = crate::meta_u64(k.value())?;
                    let tx_id = as_hash32(v.value())?;
                    items.push(self.resolve_tx(&txs, tx_id)?);
                    last_gidx = Some(gidx);
                }
            }
            Dir::Desc => {
                let hi = cursor.map(k_u64);
                let hi_bound = match &hi {
                    Some(h) => Bound::Excluded(h.as_slice()),
                    None => Bound::Unbounded,
                };
                for item in by_gidx.range::<&[u8]>((Bound::Unbounded, hi_bound))?.rev() {
                    if items.len() >= limit {
                        break;
                    }
                    let (k, v) = item?;
                    let gidx = crate::meta_u64(k.value())?;
                    let tx_id = as_hash32(v.value())?;
                    items.push(self.resolve_tx(&txs, tx_id)?);
                    last_gidx = Some(gidx);
                }
            }
        }
        let next_cursor = if items.len() == limit {
            last_gidx
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    /// Every tx of the block at `height`, in in-block order, via the block's `first_tx_gidx`
    /// and `tx_count` (see `HeaderRow` amendment in Task 6).
    pub fn txs_in_block(&self, height: u32) -> Result<Vec<(Hash32, TxRow)>, StoreError> {
        let Some(header) = self.header_at(height)? else {
            return Ok(vec![]);
        };
        let by_gidx = self.txn.open_table(TX_BY_GIDX)?;
        let txs = self.txn.open_table(TXS)?;
        let mut out = Vec::with_capacity(header.tx_count as usize);
        for i in 0..header.tx_count as u64 {
            let gidx = header.first_tx_gidx + i;
            let v = by_gidx
                .get(k_u64(gidx).as_slice())?
                .ok_or(StoreError::Corrupt("dangling tx gidx in block range"))?;
            let tx_id = as_hash32(v.value())?;
            out.push(self.resolve_tx(&txs, tx_id)?);
        }
        Ok(out)
    }

    pub fn box_by_id(&self, id: &Hash32) -> Result<Option<BoxRow>, StoreError> {
        let table = self.txn.open_table(BOXES)?;
        match table.get(id.as_slice())? {
            Some(v) => Ok(Some(BoxRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    /// Outputs of `tx` (whose id is `tx_id`), in index order, via `BOX_BY_GIDX`'s contiguous
    /// `[first_out_gidx, first_out_gidx + output_count)` range.
    pub fn boxes_of_tx(
        &self,
        tx: &TxRow,
        tx_id: &Hash32,
    ) -> Result<Vec<(Hash32, BoxRow)>, StoreError> {
        let box_by_gidx = self.txn.open_table(BOX_BY_GIDX)?;
        let boxes = self.txn.open_table(BOXES)?;
        let mut out = Vec::with_capacity(tx.output_count as usize);
        for i in 0..tx.output_count as u64 {
            let gidx = tx.first_out_gidx + i;
            let v = box_by_gidx
                .get(k_u64(gidx).as_slice())?
                .ok_or(StoreError::Corrupt("dangling box gidx in tx range"))?;
            let box_id = as_hash32(v.value())?;
            let row = boxes
                .get(box_id.as_slice())?
                .map(|v| BoxRow::decode(v.value()))
                .transpose()?
                .ok_or(StoreError::Corrupt("dangling box_by_gidx entry"))?;
            if row.tx_id != *tx_id {
                return Err(StoreError::Corrupt(
                    "box_by_gidx entry belongs to a different tx",
                ));
            }
            out.push((box_id, row));
        }
        Ok(out)
    }

    pub fn tree_row(&self, tree: &Hash32) -> Result<Option<TreeRow>, StoreError> {
        let table = self.txn.open_table(ERGO_TREES)?;
        match table.get(tree.as_slice())? {
            Some(v) => Ok(Some(TreeRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    /// Derives the tree hash for `address` (mainnet) and looks it up in `ERGO_TREES`. An
    /// unparseable address and one whose tree was never indexed both return `Ok(None)`: the
    /// two cases are not distinguishable through this API.
    pub fn tree_by_address(&self, address: &str) -> Result<Option<Hash32>, StoreError> {
        let Ok(addr) = AddressEncoder::new(NetworkPrefix::Mainnet).parse_address_from_str(address)
        else {
            return Ok(None);
        };
        let Ok(tree) = addr.script() else {
            return Ok(None);
        };
        let Ok(tree_bytes) = tree.sigma_serialize_bytes() else {
            return Ok(None);
        };
        let hash = xp_wire::tree_hash(&tree_bytes).0;
        let table = self.txn.open_table(ERGO_TREES)?;
        Ok(table.get(hash.as_slice())?.map(|_| hash))
    }

    pub fn balance(&self, tree: &Hash32) -> Result<Option<BalanceRow>, StoreError> {
        let table = self.txn.open_table(TREE_BALANCE)?;
        match table.get(tree.as_slice())? {
            Some(v) => Ok(Some(BalanceRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    pub(crate) fn resolve_box(
        &self,
        boxes: &impl ReadableTable<&'static [u8], &'static [u8]>,
        box_id: Hash32,
    ) -> Result<(Hash32, BoxRow), StoreError> {
        let row = boxes
            .get(box_id.as_slice())?
            .map(|v| BoxRow::decode(v.value()))
            .transpose()?
            .ok_or(StoreError::Corrupt("dangling gidx->box_id entry"))?;
        Ok((box_id, row))
    }

    /// Generic pager over a composite `(prefix, gidx)`-keyed index table (`TREE_BOXES`,
    /// `TREE_UNSPENT`, `TREE_TXS`, `TOKEN_BOXES`, `REGISTER_IDX`, ...): computes `(lo, hi)`
    /// from `prefix_range(prefix)`, tightens it by `cursor` (`Asc`: lower bound `cursor+1` inclusive; `Desc`: upper bound
    /// `cursor` exclusive), walks the range (`.rev()` for `Desc`), and resolves each entry's
    /// trailing gidx via `resolve`. `next_cursor` is set only when the page came back full
    /// (`items.len() == limit`), and `limit == 0` short-circuits to an empty page.
    ///
    /// `prefix` is a byte slice rather than a hash so the same pager serves `REGISTER_IDX`,
    /// whose prefix is the 33-byte `(reg, value_hash)` head.
    pub(crate) fn page_composite<T>(
        &self,
        table: Tbl,
        prefix: &[u8],
        cursor: Option<Gidx>,
        limit: usize,
        dir: Dir,
        mut resolve: impl FnMut(&Self, Gidx) -> Result<T, StoreError>,
    ) -> Result<Page<T>, StoreError> {
        if limit == 0 {
            return Ok(Page {
                items: vec![],
                next_cursor: None,
            });
        }
        let index = self.txn.open_table(table)?;
        let (lo, hi) = prefix_range(prefix);
        let mut items = Vec::new();
        let mut last_gidx = None;
        match dir {
            Dir::Asc => {
                let lo_key = match cursor {
                    Some(c) => k_prefix_gidx(prefix, c.saturating_add(1)),
                    None => lo,
                };
                for item in index.range::<&[u8]>((
                    Bound::Included(lo_key.as_slice()),
                    Bound::Excluded(hi.as_slice()),
                ))? {
                    if items.len() >= limit {
                        break;
                    }
                    let (k, _) = item?;
                    let gidx = crate::keys::gidx_of_composite(k.value())?;
                    items.push(resolve(self, gidx)?);
                    last_gidx = Some(gidx);
                }
            }
            Dir::Desc => {
                let hi_key = match cursor {
                    Some(c) => k_prefix_gidx(prefix, c),
                    None => hi,
                };
                for item in index
                    .range::<&[u8]>((
                        Bound::Included(lo.as_slice()),
                        Bound::Excluded(hi_key.as_slice()),
                    ))?
                    .rev()
                {
                    if items.len() >= limit {
                        break;
                    }
                    let (k, _) = item?;
                    let gidx = crate::keys::gidx_of_composite(k.value())?;
                    items.push(resolve(self, gidx)?);
                    last_gidx = Some(gidx);
                }
            }
        }
        let next_cursor = if items.len() == limit {
            last_gidx
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    pub fn tree_boxes(
        &self,
        tree: &Hash32,
        unspent_only: bool,
        cursor: Option<Gidx>,
        limit: usize,
        dir: Dir,
    ) -> Result<Page<(Hash32, BoxRow)>, StoreError> {
        let table = if unspent_only {
            TREE_UNSPENT
        } else {
            TREE_BOXES
        };
        self.page_composite(table, tree.as_slice(), cursor, limit, dir, |r, gidx| {
            r.box_of_gidx(gidx)
        })
    }

    pub fn tree_txs(
        &self,
        tree: &Hash32,
        cursor: Option<Gidx>,
        limit: usize,
        dir: Dir,
    ) -> Result<Page<(Hash32, TxRow)>, StoreError> {
        self.page_composite(TREE_TXS, tree.as_slice(), cursor, limit, dir, |r, gidx| {
            let tx_by_gidx = r.txn.open_table(TX_BY_GIDX)?;
            let txs = r.txn.open_table(TXS)?;
            let tx_id = as_hash32(tx_by_gidx_lookup(&tx_by_gidx, gidx)?.as_slice())?;
            r.resolve_tx(&txs, tx_id)
        })
    }

    /// Richest trees by nano-erg balance, descending, capped at `limit`. With a cursor,
    /// resumes strictly below the `(nano, tree)` pair of the last item previously returned.
    #[allow(clippy::type_complexity)]
    pub fn richlist(
        &self,
        cursor: Option<(u64, Hash32)>,
        limit: usize,
    ) -> Result<(Vec<(Hash32, u64)>, Option<(u64, Hash32)>), StoreError> {
        let table = self.txn.open_table(RICH)?;
        let hi = cursor.map(|(nano, tree)| k_rich(nano, &tree));
        let hi_bound = match &hi {
            Some(h) => Bound::Excluded(h.as_slice()),
            None => Bound::Unbounded,
        };
        let mut items = Vec::new();
        let mut last = None;
        for item in table.range::<&[u8]>((Bound::Unbounded, hi_bound))?.rev() {
            if items.len() >= limit {
                break;
            }
            let (k, _) = item?;
            let key = k.value();
            if key.len() != 40 {
                return Err(StoreError::Corrupt("bad rich key width"));
            }
            let nano = u64::from_be_bytes(
                key[..8]
                    .try_into()
                    .map_err(|_| StoreError::Corrupt("bad rich key width"))?,
            );
            let tree = as_hash32(&key[8..])?;
            items.push((tree, nano));
            last = Some((nano, tree));
        }
        let next_cursor = if items.len() == limit { last } else { None };
        Ok((items, next_cursor))
    }

    /// Box ids maturing for rent at heights `[from_height, from_height + span)`, oldest first,
    /// capped at `limit` results.
    pub fn rent_matures_range(
        &self,
        from_height: u32,
        span: u32,
        limit: usize,
    ) -> Result<Vec<(u32, Hash32)>, StoreError> {
        let table = self.txn.open_table(RENT_MATURES)?;
        let lo = k_rent(from_height, 0);
        let hi = k_rent(from_height.saturating_add(span), 0);
        let mut out = Vec::new();
        for item in table.range(lo.as_slice()..hi.as_slice())? {
            if out.len() >= limit {
                break;
            }
            let (k, v) = item?;
            let key = k.value();
            let height = rent_key_height(key)?;
            let id = as_hash32(v.value())?;
            out.push((height, id));
        }
        Ok(out)
    }

    /// Every `RENT_MATURES` entry with `mature_height <= at_height`, ascending by
    /// `(mature_height, gidx)`; `cursor` (when given) is exclusive.
    #[allow(clippy::type_complexity)]
    pub fn rent_eligible(
        &self,
        at_height: u32,
        cursor: Option<(u32, Gidx)>,
        limit: usize,
    ) -> Result<(Vec<(u32, Hash32)>, Option<(u32, Gidx)>), StoreError> {
        let table = self.txn.open_table(RENT_MATURES)?;
        let lo = match cursor {
            Some((mature, gidx)) => k_rent(mature, gidx.saturating_add(1)),
            None => k_rent(0, 0),
        };
        // Upper bound: everything up to and including `at_height` — use `at_height + 1` as
        // the exclusive bound, saturating so `at_height == u32::MAX` still covers it.
        let hi = k_rent(at_height.saturating_add(1), 0);
        let mut items = Vec::new();
        let mut last = None;
        for item in table.range(lo.as_slice()..hi.as_slice())? {
            if items.len() >= limit {
                break;
            }
            let (k, v) = item?;
            let key = k.value();
            let mature = rent_key_height(key)?;
            let gidx = crate::keys::gidx_of_composite(key)?;
            let id = as_hash32(v.value())?;
            items.push((mature, id));
            last = Some((mature, gidx));
        }
        let next_cursor = if items.len() == limit { last } else { None };
        Ok((items, next_cursor))
    }
}

/// Leading big-endian maturity height of a `RENT_MATURES` key (see `keys::k_rent`), failing
/// with [`StoreError::Corrupt`] rather than panicking on a short key.
fn rent_key_height(key: &[u8]) -> Result<u32, StoreError> {
    let head = key
        .get(..4)
        .ok_or(StoreError::Corrupt("rent key shorter than 4 bytes"))?;
    crate::meta_u32(head)
}

pub(crate) fn box_by_gidx_lookup(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    gidx: Gidx,
) -> Result<Vec<u8>, StoreError> {
    table
        .get(k_u64(gidx).as_slice())?
        .map(|v| v.value().to_vec())
        .ok_or(StoreError::Corrupt(
            "dangling tree index -> box_by_gidx entry",
        ))
}

fn tx_by_gidx_lookup(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    gidx: Gidx,
) -> Result<Vec<u8>, StoreError> {
    table
        .get(k_u64(gidx).as_slice())?
        .map(|v| v.value().to_vec())
        .ok_or(StoreError::Corrupt(
            "dangling tree index -> tx_by_gidx entry",
        ))
}
