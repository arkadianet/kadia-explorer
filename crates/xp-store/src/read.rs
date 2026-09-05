//! Minimal read-side surface needed to compile Task 4's tests.
//!
//! This is intentionally a stub: only the three methods exercised by
//! `tests/apply.rs` are implemented. Task 6 fills in the rest of the reader.

use crate::rows::{BalanceRow, BoxRow};
use crate::tables::{BOXES, RENT_MATURES, TREE_BALANCE};
use crate::{keys, Store, StoreError};
use redb::ReadTransaction;
use xp_types::Hash32;

pub struct Reader {
    txn: ReadTransaction,
}

impl Reader {
    pub fn new(store: &Store) -> Result<Reader, StoreError> {
        Ok(Reader {
            txn: store.begin_read()?,
        })
    }

    pub fn box_by_id(&self, id: &Hash32) -> Result<Option<BoxRow>, StoreError> {
        let table = self.txn.open_table(BOXES)?;
        match table.get(id.as_slice())? {
            Some(v) => Ok(Some(BoxRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn balance(&self, tree: &Hash32) -> Result<Option<BalanceRow>, StoreError> {
        let table = self.txn.open_table(TREE_BALANCE)?;
        match table.get(tree.as_slice())? {
            Some(v) => Ok(Some(BalanceRow::decode(v.value())?)),
            None => Ok(None),
        }
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
        let lo = keys::k_rent(from_height, 0);
        let hi = keys::k_rent(from_height.saturating_add(span), 0);
        let mut out = Vec::new();
        for item in table.range(lo.as_slice()..hi.as_slice())? {
            if out.len() >= limit {
                break;
            }
            let (k, v) = item?;
            let key = k.value();
            let height = crate::meta_u32(&key[..4])?;
            let id: Hash32 = v
                .value()
                .try_into()
                .map_err(|_| StoreError::Corrupt("bad rent_matures value"))?;
            out.push((height, id));
        }
        Ok(out)
    }
}
