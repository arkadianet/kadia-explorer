//! Schema-v2 `Reader` queries: tokens, holders, script templates and register search.
//!
//! A second `impl Reader` block, split out of `read.rs` purely for file size — every method
//! here reads through the same snapshot transaction and follows the same conventions:
//! `Page`/tuple cursors are exclusive and only set when a page came back full, and a
//! secondary index entry pointing at a row that is not there is [`StoreError::Corrupt`]
//! rather than a silently skipped item.

use crate::keys::{k_by_count, k_token_holder, k_u64, prefix_range};
use crate::read::{as_hash32, BoxResolver, Dir, Page};
use crate::rows::{BoxRow, TemplateRow, TokenRow};
use crate::tables::*;
use crate::{Reader, StoreError};
use redb::ReadableTable;
use std::ops::Bound;
use xp_types::{Gidx, Hash32};

/// Splits a `TOKEN_HOLDERS` key (`token 32 ++ amount u64 BE ++ tree 32`) into its amount and
/// tree components.
fn holder_key_parts(key: &[u8]) -> Result<(u64, Hash32), StoreError> {
    if key.len() != 72 {
        return Err(StoreError::Corrupt("bad token holder key width"));
    }
    let amount = crate::meta_u64(&key[32..40])?;
    Ok((amount, as_hash32(&key[40..])?))
}

/// Splits a `TOKENS_BY_HOLDERS` key (`count u64 BE ++ token 32`) into its components.
fn by_count_key_parts(key: &[u8]) -> Result<(u64, Hash32), StoreError> {
    if key.len() != 40 {
        return Err(StoreError::Corrupt("bad tokens-by-holders key width"));
    }
    let count = crate::meta_u64(&key[..8])?;
    Ok((count, as_hash32(&key[8..])?))
}

/// The `(reg, value_hash)` head shared by every `REGISTER_IDX` key for one register value.
fn register_prefix(reg: u8, value_hash: &Hash32) -> Vec<u8> {
    let mut p = Vec::with_capacity(33);
    p.push(reg);
    p.extend_from_slice(value_hash);
    p
}

impl Reader {
    pub fn token(&self, id: &Hash32) -> Result<Option<TokenRow>, StoreError> {
        let table = self.txn.open_table(TOKENS)?;
        match table.get(id.as_slice())? {
            Some(v) => Ok(Some(TokenRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    /// The [`TokenRow`] for `id`, which a secondary index promised exists.
    fn resolve_token(
        &self,
        tokens: &impl ReadableTable<&'static [u8], &'static [u8]>,
        id: Hash32,
    ) -> Result<(Hash32, TokenRow), StoreError> {
        let row = tokens
            .get(id.as_slice())?
            .map(|v| TokenRow::decode(v.value()))
            .transpose()?
            .ok_or(StoreError::Corrupt("dangling token index entry"))?;
        Ok((id, row))
    }

    /// Tokens newest-mint-first, via `TOKENS_BY_GIDX`. The cursor is the mint gidx of the
    /// last item returned and is exclusive.
    pub fn tokens_newest(
        &self,
        cursor: Option<Gidx>,
        limit: usize,
    ) -> Result<Page<(Hash32, TokenRow)>, StoreError> {
        if limit == 0 {
            return Ok(Page {
                items: vec![],
                next_cursor: None,
            });
        }
        let by_gidx = self.txn.open_table(TOKENS_BY_GIDX)?;
        let tokens = self.txn.open_table(TOKENS)?;
        let hi = cursor.map(k_u64);
        let hi_bound = match &hi {
            Some(h) => Bound::Excluded(h.as_slice()),
            None => Bound::Unbounded,
        };
        let mut items = Vec::new();
        let mut last_gidx = None;
        for item in by_gidx.range::<&[u8]>((Bound::Unbounded, hi_bound))?.rev() {
            if items.len() >= limit {
                break;
            }
            let (k, v) = item?;
            let gidx = crate::meta_u64(k.value())?;
            let id = as_hash32(v.value())?;
            items.push(self.resolve_token(&tokens, id)?);
            last_gidx = Some(gidx);
        }
        let next_cursor = if items.len() == limit {
            last_gidx
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    /// Tokens by descending holder count, via `TOKENS_BY_HOLDERS`. The cursor is the
    /// `(holder_count, token_id)` pair of the last item returned and is exclusive — the id
    /// is part of it because counts are not unique.
    #[allow(clippy::type_complexity)]
    pub fn tokens_by_holders(
        &self,
        cursor: Option<(u64, Hash32)>,
        limit: usize,
    ) -> Result<(Vec<(Hash32, TokenRow)>, Option<(u64, Hash32)>), StoreError> {
        if limit == 0 {
            return Ok((vec![], None));
        }
        let index = self.txn.open_table(TOKENS_BY_HOLDERS)?;
        let tokens = self.txn.open_table(TOKENS)?;
        let hi = cursor.map(|(count, id)| k_by_count(count, &id));
        let hi_bound = match &hi {
            Some(h) => Bound::Excluded(h.as_slice()),
            None => Bound::Unbounded,
        };
        let mut items = Vec::new();
        let mut last = None;
        for item in index.range::<&[u8]>((Bound::Unbounded, hi_bound))?.rev() {
            if items.len() >= limit {
                break;
            }
            let (k, _) = item?;
            let (count, id) = by_count_key_parts(k.value())?;
            items.push(self.resolve_token(&tokens, id)?);
            last = Some((count, id));
        }
        let next_cursor = if items.len() == limit { last } else { None };
        Ok((items, next_cursor))
    }

    /// A token's holders as `(tree_hash, amount)`, largest amount first, via the
    /// `(token, amount, tree)` ordering of `TOKEN_HOLDERS`. The cursor is the
    /// `(amount, tree)` pair of the last item returned and is exclusive.
    #[allow(clippy::type_complexity)]
    pub fn token_holders(
        &self,
        id: &Hash32,
        cursor: Option<(u64, Hash32)>,
        limit: usize,
    ) -> Result<(Vec<(Hash32, u64)>, Option<(u64, Hash32)>), StoreError> {
        if limit == 0 {
            return Ok((vec![], None));
        }
        let index = self.txn.open_table(TOKEN_HOLDERS)?;
        let (lo, hi) = prefix_range(id.as_slice());
        let hi_key = match cursor {
            Some((amount, tree)) => k_token_holder(id, amount, &tree).to_vec(),
            None => hi,
        };
        let mut items = Vec::new();
        let mut last = None;
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
            let (amount, tree) = holder_key_parts(k.value())?;
            items.push((tree, amount));
            last = Some((amount, tree));
        }
        let next_cursor = if items.len() == limit { last } else { None };
        Ok((items, next_cursor))
    }

    /// Boxes carrying `id`: every one ever, or only those still unspent.
    pub fn token_boxes(
        &self,
        id: &Hash32,
        unspent_only: bool,
        cursor: Option<Gidx>,
        limit: usize,
        dir: Dir,
    ) -> Result<Page<(Hash32, BoxRow)>, StoreError> {
        let table = if unspent_only {
            TOKEN_UNSPENT
        } else {
            TOKEN_BOXES
        };
        let boxes = BoxResolver::open(&self.txn)?;
        self.page_composite(table, id.as_slice(), cursor, limit, dir, |gidx| {
            boxes.get(gidx)
        })
    }

    pub fn template(&self, hash: &Hash32) -> Result<Option<TemplateRow>, StoreError> {
        let table = self.txn.open_table(TEMPLATES)?;
        match table.get(hash.as_slice())? {
            Some(v) => Ok(Some(TemplateRow::decode(v.value())?)),
            None => Ok(None),
        }
    }

    /// Boxes whose ergo tree has this template hash — the same contract whatever its
    /// segregated constants — either all of them or only the unspent ones.
    pub fn template_boxes(
        &self,
        hash: &Hash32,
        unspent_only: bool,
        cursor: Option<Gidx>,
        limit: usize,
        dir: Dir,
    ) -> Result<Page<(Hash32, BoxRow)>, StoreError> {
        let table = if unspent_only {
            TEMPLATE_UNSPENT
        } else {
            TEMPLATE_BOXES
        };
        let boxes = BoxResolver::open(&self.txn)?;
        self.page_composite(table, hash.as_slice(), cursor, limit, dir, |gidx| {
            boxes.get(gidx)
        })
    }

    /// Boxes whose register `reg` (4..=9) holds the value with this blake2b256 hash, via the
    /// `(reg, value_hash, gidx)` prefix of `REGISTER_IDX`. A register outside 4..=9 is never
    /// indexed, so it simply yields an empty page.
    pub fn boxes_by_register(
        &self,
        reg: u8,
        value_hash: &Hash32,
        cursor: Option<Gidx>,
        limit: usize,
        dir: Dir,
    ) -> Result<Page<(Hash32, BoxRow)>, StoreError> {
        let prefix = register_prefix(reg, value_hash);
        let boxes = BoxResolver::open(&self.txn)?;
        self.page_composite(REGISTER_IDX, &prefix, cursor, limit, dir, |gidx| {
            boxes.get(gidx)
        })
    }

    /// `(id, name, decimals)` for those `ids` that have a `TOKENS` row, in the order given.
    ///
    /// The result is **neither index-aligned with `ids` nor deduplicated**: an id without a
    /// row is skipped (so `out.len() <= ids.len()` and `out[i]` need not describe `ids[i]`),
    /// and a known id repeated in `ids` is returned once per occurrence. Callers enriching a
    /// box's token list should collect it into a `HashMap<Hash32, _>` and look ids up there,
    /// never index into it positionally.
    ///
    /// Ids are skipped rather than reported missing because a partial store legitimately holds
    /// boxes carrying tokens minted before its seed height.
    pub fn token_names(
        &self,
        ids: &[Hash32],
    ) -> Result<Vec<(Hash32, String, Option<u8>)>, StoreError> {
        let tokens = self.txn.open_table(TOKENS)?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(v) = tokens.get(id.as_slice())? else {
                continue;
            };
            let row = TokenRow::decode(v.value())?;
            out.push((*id, row.name, row.decimals));
        }
        Ok(out)
    }
}
