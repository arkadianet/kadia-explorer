//! Rebuildable token-name index. Auxiliary tables deliberately do not belong to canonical
//! `tables::ALL`: building this view changes neither consensus rows nor their fingerprint.
//! V1 folds ASCII case/whitespace only. Non-ASCII UTF-8 is preserved exactly; Unicode case,
//! normalization forms and visually similar characters are never treated as equivalent.

use crate::keys::{k_u32, prefix_range};
use crate::read::as_hash32;
use crate::rows::{HeaderRow, TokenRow};
use crate::tables::*;
use crate::{Reader, Store, StoreError};
use redb::{ReadableTable, ReadableTableMetadata, TableDefinition, WriteTransaction};
use std::ops::Bound;
use xp_types::Hash32;

pub const TOKEN_NAME_INDEX_VERSION: u32 = 1;
pub const MAX_TOKEN_NAME_QUERY_BYTES: usize = 512;
pub const MAX_TOKEN_NAME_BYTES: usize = 4096;
pub const MAX_TOKEN_NAME_RESULTS: usize = 100;
pub const MAX_TOKEN_NAME_BATCH: usize = 1000;
pub const MAX_TOKEN_NAME_SEARCH_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TOKEN_NAME_BATCH_BYTES: usize = 8 * 1024 * 1024;

pub const TOKEN_NAMES: Tbl = TableDefinition::new("aux_token_names");
pub const TOKEN_NAME_META: Tbl = TableDefinition::new("aux_token_name_meta");
const STATE_KEY: &[u8] = b"state";
const CLEARING: u8 = 0;
const BUILDING: u8 = 1;
const READY: u8 = 2;
const ANCHOR_LEN: usize = 61;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenNameMatch {
    Exact,
    Prefix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenNameIndexStatus {
    pub version: u32,
    pub ready: bool,
    pub phase: &'static str,
    /// Rows examined by this build; deletions can make this exceed the current total.
    pub scanned_tokens: u64,
    pub total_tokens: u64,
    pub indexed_names: u64,
    /// Exact only when ready. Includes unnamed, control-bearing and oversized names.
    pub unindexed_tokens: u64,
}

#[derive(Debug)]
pub struct TokenNameSearchPage {
    pub items: Vec<(Hash32, TokenRow)>,
    pub next_cursor: Option<Hash32>,
    pub normalized_query: String,
}

pub fn normalize_token_name(value: &str) -> Result<String, StoreError> {
    normalize(value, MAX_TOKEN_NAME_QUERY_BYTES)
}

fn normalize(value: &str, max: usize) -> Result<String, StoreError> {
    if value.len() > max {
        return Err(StoreError::InvalidTokenSearch(
            "name exceeds UTF-8 byte limit",
        ));
    }
    if value
        .bytes()
        .any(|b| b.is_ascii_control() && !b.is_ascii_whitespace())
    {
        return Err(StoreError::InvalidTokenSearch(
            "name contains ASCII control characters",
        ));
    }
    let normalized = value
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(StoreError::InvalidTokenSearch("name is empty"));
    }
    Ok(normalized)
}

fn index_key(name: &str, id: &Hash32) -> Result<Vec<u8>, StoreError> {
    let mut key = normalize(name, MAX_TOKEN_NAME_BYTES)?.into_bytes();
    key.push(0);
    key.extend_from_slice(id);
    Ok(key)
}

pub(crate) fn insert_name(
    txn: &WriteTransaction,
    id: &Hash32,
    name: &str,
) -> Result<(), StoreError> {
    if let Ok(key) = index_key(name, id) {
        txn.open_table(TOKEN_NAMES)?
            .insert(key.as_slice(), &[][..])?;
    }
    Ok(())
}

pub(crate) fn remove_name(
    txn: &WriteTransaction,
    id: &Hash32,
    name: &str,
) -> Result<(), StoreError> {
    if let Ok(key) = index_key(name, id) {
        txn.open_table(TOKEN_NAMES)?.remove(key.as_slice())?;
    }
    Ok(())
}

#[derive(Clone)]
struct State {
    version: u32,
    phase: u8,
    scanned: u64,
    cursor: Option<Hash32>,
    anchor: [u8; ANCHOR_LEN],
}

impl State {
    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 107 || bytes[4] > READY || bytes[13] > 1 {
            return None;
        }
        Some(Self {
            version: u32::from_be_bytes(bytes[..4].try_into().ok()?),
            phase: bytes[4],
            scanned: u64::from_be_bytes(bytes[5..13].try_into().ok()?),
            cursor: (bytes[13] == 1).then(|| bytes[14..46].try_into().expect("fixed width")),
            anchor: bytes[46..].try_into().ok()?,
        })
    }

    fn encode(&self) -> [u8; 107] {
        let mut out = [0; 107];
        out[..4].copy_from_slice(&self.version.to_be_bytes());
        out[4] = self.phase;
        out[5..13].copy_from_slice(&self.scanned.to_be_bytes());
        if let Some(cursor) = self.cursor {
            out[13] = 1;
            out[14..46].copy_from_slice(&cursor);
        }
        out[46..].copy_from_slice(&self.anchor);
        out
    }
}

fn anchor(
    meta: &impl ReadableTable<&'static [u8], &'static [u8]>,
    headers: &impl ReadableTable<&'static [u8], &'static [u8]>,
    total: u64,
) -> Result<Option<[u8; ANCHOR_LEN]>, StoreError> {
    let mut bytes = [0; ANCHOR_LEN];
    if let Some(height) = meta.get(META_INDEXED_HEIGHT)? {
        let height = crate::meta_u32(height.value())?;
        // Some partial stores record a starting height before their first known header.
        // A missing in-range header is corruption for canonical readers/writers to report;
        // an auxiliary view must not prevent those readers from opening the store.
        let Some(row) = headers.get(k_u32(height).as_slice())? else {
            return Ok(None);
        };
        bytes[0] = 1;
        bytes[1..5].copy_from_slice(&height.to_be_bytes());
        bytes[5..37].copy_from_slice(&HeaderRow::decode(row.value())?.id);
    }
    for (key, start) in [(META_NEXT_BOX_GIDX, 37), (META_NEXT_TX_GIDX, 45)] {
        let value = meta
            .get(key)?
            .map(|v| crate::meta_u64(v.value()))
            .transpose()?
            .unwrap_or(0);
        bytes[start..start + 8].copy_from_slice(&value.to_be_bytes());
    }
    bytes[53..].copy_from_slice(&total.to_be_bytes());
    Ok(Some(bytes))
}

fn write_anchor(txn: &WriteTransaction) -> Result<Option<[u8; ANCHOR_LEN]>, StoreError> {
    anchor(
        &txn.open_table(META)?,
        &txn.open_table(HEADERS)?,
        txn.open_table(TOKENS)?.len()?,
    )
}

fn load_state(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
) -> Result<Option<State>, StoreError> {
    Ok(table.get(STATE_KEY)?.and_then(|v| State::decode(v.value())))
}

fn save_state(txn: &WriteTransaction, state: &State) -> Result<(), StoreError> {
    txn.open_table(TOKEN_NAME_META)?
        .insert(STATE_KEY, state.encode().as_slice())?;
    Ok(())
}

/// Must precede every core mutation. A changed anchor invalidates the complete view before
/// the new writer updates it, so an old binary's missing writes cannot be certified ready.
pub(crate) fn prepare_write(txn: &WriteTransaction) -> Result<(), StoreError> {
    let entries = txn.open_table(TOKEN_NAMES)?.len()?;
    let state = load_state(&txn.open_table(TOKEN_NAME_META)?)?;
    let Some(current) = write_anchor(txn)? else {
        txn.open_table(TOKEN_NAME_META)?.remove(STATE_KEY)?;
        return Ok(());
    };
    if state.is_some_and(|s| s.version == TOKEN_NAME_INDEX_VERSION && s.anchor == current) {
        return Ok(());
    }
    let total = crate::meta_u64(&current[53..])?;
    save_state(
        txn,
        &State {
            version: TOKEN_NAME_INDEX_VERSION,
            phase: if entries > 0 {
                CLEARING
            } else if total > 0 {
                BUILDING
            } else {
                READY
            },
            scanned: 0,
            cursor: None,
            anchor: current,
        },
    )
}

/// Called only after prepare_write in the same transaction and atomic name maintenance.
pub(crate) fn finish_write(txn: &WriteTransaction) -> Result<(), StoreError> {
    let Some(current) = write_anchor(txn)? else {
        txn.open_table(TOKEN_NAME_META)?.remove(STATE_KEY)?;
        return Ok(());
    };
    let Some(mut state) = load_state(&txn.open_table(TOKEN_NAME_META)?)? else {
        // A previously headerless store has acquired a canonical anchor. Its historical
        // coverage is unknown, so initialize a rebuild rather than certify only new mints.
        return prepare_write(txn);
    };
    state.anchor = current;
    save_state(txn, &state)
}

impl Store {
    /// At most `max_rows` index removals or token rows and 8 MiB of encoded TokenRows in
    /// one atomic batch. Restart-safe; regular apply/rollback may interleave between batches.
    pub fn backfill_token_names_batch(
        &self,
        max_rows: usize,
    ) -> Result<TokenNameIndexStatus, StoreError> {
        if max_rows == 0 || max_rows > MAX_TOKEN_NAME_BATCH {
            return Err(StoreError::InvalidTokenSearch(
                "backfill batch must contain 1..=1000 rows",
            ));
        }
        let txn = self.db.begin_write()?;
        prepare_write(&txn)?;
        let mut state = load_state(&txn.open_table(TOKEN_NAME_META)?)?
            .ok_or(StoreError::TokenSearchNotReady)?;
        if state.phase == CLEARING {
            let mut index = txn.open_table(TOKEN_NAMES)?;
            let mut bytes = 0;
            for _ in 0..max_rows {
                let size = index
                    .iter()?
                    .next()
                    .transpose()?
                    .map(|(key, _)| key.value().len());
                let Some(size) = size else {
                    break;
                };
                if size > MAX_TOKEN_NAME_BATCH_BYTES {
                    return Err(StoreError::ReadLimit("token_backfill_bytes"));
                }
                if bytes + size > MAX_TOKEN_NAME_BATCH_BYTES {
                    break;
                }
                bytes += size;
                index.pop_first()?;
            }
            if index.is_empty()? {
                state.phase = BUILDING;
            }
        } else if state.phase == BUILDING {
            let tokens = txn.open_table(TOKENS)?;
            let mut bytes = 0;
            let cursor = state.cursor;
            let lower = cursor
                .as_ref()
                .map_or(Bound::Unbounded, |id| Bound::Excluded(id.as_slice()));
            let mut exhausted = true;
            for (visited, entry) in tokens
                .range::<&[u8]>((lower, Bound::Unbounded))?
                .enumerate()
            {
                if visited == max_rows {
                    exhausted = false;
                    break;
                }
                let (id, value) = entry?;
                let size = value.value().len();
                if size > MAX_TOKEN_NAME_BATCH_BYTES {
                    return Err(StoreError::ReadLimit("token_backfill_bytes"));
                }
                if bytes + size > MAX_TOKEN_NAME_BATCH_BYTES {
                    exhausted = false;
                    break;
                }
                bytes += size;
                let id = as_hash32(id.value())?;
                let row = TokenRow::decode(value.value())?;
                insert_name(&txn, &id, &row.name)?;
                state.cursor = Some(id);
                state.scanned = state.scanned.saturating_add(1);
            }
            if exhausted {
                state.phase = READY;
            }
        }
        save_state(&txn, &state)?;
        txn.commit()?;
        Reader::new(self)?.token_name_index_status()
    }
}

impl Reader {
    pub fn token_name_index_status(&self) -> Result<TokenNameIndexStatus, StoreError> {
        let total_tokens = self.txn.open_table(TOKENS)?.len()?;
        let current = anchor(
            &self.txn.open_table(META)?,
            &self.txn.open_table(HEADERS)?,
            total_tokens,
        )?;
        let state = load_state(&self.txn.open_table(TOKEN_NAME_META)?)?;
        let state =
            state.filter(|s| s.version == TOKEN_NAME_INDEX_VERSION && Some(s.anchor) == current);
        let indexed_names = if state.as_ref().is_some_and(|s| s.phase != CLEARING) {
            self.txn.open_table(TOKEN_NAMES)?.len()?
        } else {
            0
        };
        let ready = state.as_ref().is_some_and(|s| s.phase == READY);
        Ok(TokenNameIndexStatus {
            version: TOKEN_NAME_INDEX_VERSION,
            ready,
            phase: match state.as_ref().map(|s| s.phase) {
                Some(CLEARING) => "clearing",
                Some(BUILDING) => "building",
                Some(READY) => "ready",
                _ => "stale",
            },
            scanned_tokens: state.map_or(0, |s| s.scanned),
            total_tokens,
            indexed_names,
            unindexed_tokens: if ready {
                total_tokens
                    .checked_sub(indexed_names)
                    .ok_or(StoreError::Corrupt("token name index count exceeds tokens"))?
            } else {
                0
            },
        })
    }

    /// Lexicographic normalized UTF-8 name, then token ID. The exclusive cursor is the
    /// previous token ID; its immutable name reconstructs the key without large cursors.
    pub fn search_token_names(
        &self,
        query: &str,
        mode: TokenNameMatch,
        cursor: Option<Hash32>,
        limit: usize,
    ) -> Result<TokenNameSearchPage, StoreError> {
        let normalized_query = normalize_token_name(query)?;
        if limit == 0 || limit > MAX_TOKEN_NAME_RESULTS {
            return Err(StoreError::InvalidTokenSearch(
                "search limit must be 1..=100",
            ));
        }
        if !self.token_name_index_status()?.ready {
            return Err(StoreError::TokenSearchNotReady);
        }
        let tokens = self.txn.open_table(TOKENS)?;
        let index = self.txn.open_table(TOKEN_NAMES)?;
        let mut prefix = normalized_query.as_bytes().to_vec();
        if mode == TokenNameMatch::Exact {
            prefix.push(0);
        }
        let (lo, hi) = prefix_range(&prefix);
        let mut bytes = 0usize;
        let mut read_row = |id: Hash32| -> Result<TokenRow, StoreError> {
            self.count_lookup(2, 1);
            let value = tokens
                .get(id.as_slice())?
                .ok_or(StoreError::InvalidTokenSearch("cursor token is absent"))?;
            bytes = bytes
                .checked_add(value.value().len())
                .ok_or(StoreError::ReadLimit("token_search_bytes"))?;
            if bytes > MAX_TOKEN_NAME_SEARCH_BYTES {
                return Err(StoreError::ReadLimit("token_search_bytes"));
            }
            TokenRow::decode(value.value())
        };
        let after = cursor
            .map(|id| {
                let row = read_row(id)?;
                let key = index_key(&row.name, &id)?;
                if !key.starts_with(&prefix) {
                    return Err(StoreError::InvalidTokenSearch(
                        "cursor does not match query",
                    ));
                }
                Ok(key)
            })
            .transpose()?;
        let lower = after
            .as_ref()
            .map_or(Bound::Included(lo.as_slice()), |key| {
                Bound::Excluded(key.as_slice())
            });
        let mut items = Vec::new();
        let mut next_cursor = None;
        for entry in index.range::<&[u8]>((lower, Bound::Excluded(hi.as_slice())))? {
            let (key, _) = entry?;
            if items.len() == limit {
                next_cursor = items.last().map(|(id, _)| *id);
                break;
            }
            let key = key.value();
            if key.len() < 34 || key.len() > MAX_TOKEN_NAME_BYTES + 33 || key[key.len() - 33] != 0 {
                return Err(StoreError::Corrupt("bad token name index key"));
            }
            let id = as_hash32(&key[key.len() - 32..])?;
            let row = read_row(id).map_err(|e| match e {
                StoreError::InvalidTokenSearch(_) => {
                    StoreError::Corrupt("dangling token name index entry")
                }
                e => e,
            })?;
            if index_key(&row.name, &id)?.as_slice() != key {
                return Err(StoreError::Corrupt("token name index mismatch"));
            }
            items.push((id, row));
        }
        Ok(TokenNameSearchPage {
            items,
            next_cursor,
            normalized_query,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readers_and_next_apply_refuse_an_index_missing_preexisting_core_writes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("x.redb")).unwrap();
        let block =
            xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/453051.json"))
                .unwrap();
        store
            .seed_for_tests(453050, block.header.parent_id.0)
            .unwrap();
        // Simulate an older writer after this process prepared its auxiliary metadata.
        let txn = store.db.begin_write().unwrap();
        let row = TokenRow {
            mint_tx: [8; 32],
            mint_box: [9; 32],
            mint_height: 1,
            mint_gidx: 1,
            name: "Missed".into(),
            description: String::new(),
            decimals: None,
            token_type: None,
            emission: 1,
            burned: 0,
            holder_count: 1,
            box_count: 1,
        };
        txn.open_table(TOKENS)
            .unwrap()
            .insert([10u8; 32].as_slice(), row.encode().as_slice())
            .unwrap();
        txn.commit().unwrap();
        let reader = Reader::new(&store).unwrap();
        assert_eq!(reader.token_name_index_status().unwrap().phase, "stale");
        assert!(matches!(
            reader.search_token_names("missed", TokenNameMatch::Exact, None, 10),
            Err(StoreError::TokenSearchNotReady)
        ));
        drop(reader);
        store.apply_batch(&[block], true).unwrap();
        assert!(
            !Reader::new(&store)
                .unwrap()
                .token_name_index_status()
                .unwrap()
                .ready
        );
        while !store.backfill_token_names_batch(1000).unwrap().ready {}
        assert_eq!(
            Reader::new(&store)
                .unwrap()
                .search_token_names("missed", TokenNameMatch::Exact, None, 10)
                .unwrap()
                .items
                .len(),
            1
        );
    }
}
