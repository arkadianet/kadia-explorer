//! Rebuildable token/transaction membership. Retained boxes prove both creation and
//! consumption, including burns. Auxiliary tables never alter canonical fingerprints.
use crate::keys::{k_hash_gidx, k_u32, prefix_range};
use crate::read::as_hash32;
use crate::rows::BoxRow;
use crate::tables::*;
use crate::{Reader, Store, StoreError};
use redb::{ReadableTable, ReadableTableMetadata, TableDefinition, WriteTransaction};
use std::ops::Bound;
use std::time::{Duration, Instant};
use xp_types::Hash32;

pub const TOKEN_HISTORY_VERSION: u32 = 1;
pub const MAX_HISTORY_BATCH: usize = 500;
pub const MAX_HISTORY_BATCH_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_HISTORY_ROW_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_HISTORY_BATCH_WORK: usize = 10_000;
pub const TOKEN_TXS: Tbl = TableDefinition::new("aux_token_txs");
pub const TX_TOKENS: Tbl = TableDefinition::new("aux_tx_tokens");
pub const TOKEN_HISTORY_META: Tbl = TableDefinition::new("aux_token_history_meta");
const STATE_KEY: &[u8] = b"state";
const CLEARING: u8 = 0;
const BUILDING: u8 = 1;
const READY: u8 = 2;
const ANCHOR_LEN: usize = 61;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenHistoryStatus {
    pub version: u32,
    pub ready: bool,
    pub phase: &'static str,
    pub scanned_boxes: u64,
    pub total_boxes: u64,
    pub indexed_memberships: u64,
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
    let mut out = [0; ANCHOR_LEN];
    if let Some(height) = meta.get(META_INDEXED_HEIGHT)? {
        let height = crate::meta_u32(height.value())?;
        let Some(header) = headers.get(k_u32(height).as_slice())? else {
            return Ok(None);
        };
        out[0] = 1;
        out[1..5].copy_from_slice(&height.to_be_bytes());
        out[5..37].copy_from_slice(
            header
                .value()
                .get(..32)
                .ok_or(StoreError::Corrupt("short token-history anchor header"))?,
        );
    }
    for (key, start) in [(META_NEXT_BOX_GIDX, 37), (META_NEXT_TX_GIDX, 45)] {
        let value = meta
            .get(key)?
            .map(|v| crate::meta_u64(v.value()))
            .transpose()?
            .unwrap_or(0);
        out[start..start + 8].copy_from_slice(&value.to_be_bytes());
    }
    out[53..].copy_from_slice(&total.to_be_bytes());
    Ok(Some(out))
}
fn current(txn: &WriteTransaction) -> Result<Option<[u8; ANCHOR_LEN]>, StoreError> {
    anchor(
        &txn.open_table(META)?,
        &txn.open_table(HEADERS)?,
        txn.open_table(BOXES)?.len()?,
    )
}
fn state(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
) -> Result<Option<State>, StoreError> {
    Ok(table.get(STATE_KEY)?.and_then(|v| State::decode(v.value())))
}
fn save(txn: &WriteTransaction, state: &State) -> Result<(), StoreError> {
    txn.open_table(TOKEN_HISTORY_META)?
        .insert(STATE_KEY, state.encode().as_slice())?;
    Ok(())
}

/// Validate before every core mutation, so old-binary gaps cannot become certified by
/// one subsequent new-binary apply/rollback. Headerless partial starts remain unavailable.
pub(crate) fn prepare_write(txn: &WriteTransaction) -> Result<(), StoreError> {
    let entries = txn.open_table(TOKEN_TXS)?.len()? + txn.open_table(TX_TOKENS)?.len()?;
    let previous = state(&txn.open_table(TOKEN_HISTORY_META)?)?;
    let Some(anchor) = current(txn)? else {
        txn.open_table(TOKEN_HISTORY_META)?.remove(STATE_KEY)?;
        return Ok(());
    };
    if previous.is_some_and(|s| s.version == TOKEN_HISTORY_VERSION && s.anchor == anchor) {
        return Ok(());
    }
    let total = crate::meta_u64(&anchor[53..])?;
    save(
        txn,
        &State {
            version: TOKEN_HISTORY_VERSION,
            phase: if entries > 0 {
                CLEARING
            } else if total > 0 {
                BUILDING
            } else {
                READY
            },
            scanned: 0,
            cursor: None,
            anchor,
        },
    )
}
pub(crate) fn finish_write(txn: &WriteTransaction) -> Result<(), StoreError> {
    let Some(anchor) = current(txn)? else {
        txn.open_table(TOKEN_HISTORY_META)?.remove(STATE_KEY)?;
        return Ok(());
    };
    let Some(mut state) = state(&txn.open_table(TOKEN_HISTORY_META)?)? else {
        return prepare_write(txn);
    };
    state.anchor = anchor;
    save(txn, &state)
}
fn reverse_key(gidx: u64, token: &Hash32) -> [u8; 40] {
    let mut key = [0; 40];
    key[..8].copy_from_slice(&gidx.to_be_bytes());
    key[8..].copy_from_slice(token);
    key
}
pub(crate) fn insert(txn: &WriteTransaction, token: &Hash32, gidx: u64) -> Result<(), StoreError> {
    txn.open_table(TOKEN_TXS)?
        .insert(k_hash_gidx(token, gidx).as_slice(), &[][..])?;
    txn.open_table(TX_TOKENS)?
        .insert(reverse_key(gidx, token).as_slice(), &[][..])?;
    Ok(())
}
/// Called while the canonical transaction still exists; reverse keys bound rollback to
/// that transaction's own token memberships, even during an interrupted backfill.
pub(crate) fn remove_tx(txn: &WriteTransaction, gidx: u64) -> Result<(), StoreError> {
    let (lo, hi) = prefix_range(&gidx.to_be_bytes());
    let mut reverse = txn.open_table(TX_TOKENS)?;
    let keys = reverse
        .range::<&[u8]>(lo.as_slice()..hi.as_slice())?
        .map(|entry| {
            let (key, _) = entry?;
            Ok(key.value().to_vec())
        })
        .collect::<Result<Vec<_>, redb::StorageError>>()?;
    let mut index = txn.open_table(TOKEN_TXS)?;
    for key in keys {
        if key.len() != 40 {
            return Err(StoreError::Corrupt("bad reverse token-history key"));
        }
        let token = as_hash32(&key[8..])?;
        index.remove(k_hash_gidx(&token, gidx).as_slice())?;
        reverse.remove(key.as_slice())?;
    }
    Ok(())
}

fn tx_position(txn: &WriteTransaction, id: &Hash32) -> Result<Option<(u64, u32)>, StoreError> {
    // Fixed fields precede TxRow's input vectors. No row copy or vector decode is needed.
    let txs = txn.open_table(TXS)?;
    let result = txs
        .get(id.as_slice())?
        .map(|v| {
            let bytes = v.value();
            let prefix = bytes
                .get(..14)
                .ok_or(StoreError::Corrupt("short token-history transaction"))?;
            Ok((
                crate::meta_u64(&prefix[6..14])?,
                crate::meta_u32(&prefix[..4])?,
            ))
        })
        .transpose();
    result
}

impl Store {
    /// Bounded, restart-safe auxiliary rebuild. No canonical rows or undo encodings change.
    /// One batch admits at most 500 boxes, 8 MiB total, 2 MiB per row, 10k token edges/work.
    pub fn backfill_token_history_batch(
        &self,
        max_rows: usize,
    ) -> Result<TokenHistoryStatus, StoreError> {
        if max_rows == 0 || max_rows > MAX_HISTORY_BATCH {
            return Err(StoreError::ReadLimit("token_history_batch_size"));
        }
        let txn = self.db.begin_write()?;
        prepare_write(&txn)?;
        let mut progress =
            state(&txn.open_table(TOKEN_HISTORY_META)?)?.ok_or(StoreError::TokenHistoryNotReady)?;
        if progress.phase == CLEARING {
            // Clear both independently: a version change may leave either direction dirty.
            let mut left = max_rows;
            for table in [TOKEN_TXS, TX_TOKENS] {
                let mut index = txn.open_table(table)?;
                while left > 0 {
                    if index.pop_first()?.is_none() {
                        break;
                    }
                    left -= 1;
                }
            }
            if txn.open_table(TOKEN_TXS)?.is_empty()? && txn.open_table(TX_TOKENS)?.is_empty()? {
                progress.phase = BUILDING;
            }
        } else if progress.phase == BUILDING {
            let boxes = txn.open_table(BOXES)?;
            let cursor = progress.cursor;
            let lower = cursor
                .as_ref()
                .map_or(Bound::Unbounded, |id| Bound::Excluded(id.as_slice()));
            let mut bytes = 0;
            let mut work = 0;
            let mut exhausted = true;
            let deadline = Instant::now() + Duration::from_millis(250);
            for (visited, entry) in boxes.range::<&[u8]>((lower, Bound::Unbounded))?.enumerate() {
                if visited == max_rows || (visited > 0 && Instant::now() > deadline) {
                    exhausted = false;
                    break;
                }
                let (id, value) = entry?;
                let size = value.value().len();
                if size > MAX_HISTORY_ROW_BYTES {
                    return Err(StoreError::ReadLimit("token_history_row_bytes"));
                }
                // BoxRow's fixed prefix is90bytes, followed by the token-vector count.
                let count = crate::meta_u32(
                    value
                        .value()
                        .get(90..94)
                        .ok_or(StoreError::Corrupt("short token-history box"))?,
                )? as usize;
                let cost = 1 + 2 * count;
                if cost > MAX_HISTORY_BATCH_WORK {
                    return Err(StoreError::ReadLimit("token_history_row_work"));
                }
                if bytes + size > MAX_HISTORY_BATCH_BYTES || work + cost > MAX_HISTORY_BATCH_WORK {
                    exhausted = false;
                    break;
                }
                bytes += size;
                work += cost;
                let box_id = as_hash32(id.value())?;
                let row = BoxRow::decode(value.value())?;
                if !row.tokens.is_empty() {
                    let origin = tx_position(&txn, &row.tx_id)?;
                    if origin.is_none() && row.tx_id != [0; 32] {
                        return Err(StoreError::Corrupt(
                            "missing token-history origin transaction",
                        ));
                    }
                    let spent = row
                        .spent
                        .map(|(id, height)| {
                            let position = tx_position(&txn, &id)?.ok_or(StoreError::Corrupt(
                                "missing token-history spending transaction",
                            ))?;
                            if position.1 != height {
                                return Err(StoreError::Corrupt(
                                    "token-history spend height mismatch",
                                ));
                            }
                            Ok(position.0)
                        })
                        .transpose()?;
                    for (token, _) in &row.tokens {
                        if let Some((gidx, _)) = origin {
                            insert(&txn, token, gidx)?;
                        }
                        if let Some(gidx) = spent {
                            insert(&txn, token, gidx)?;
                        }
                    }
                }
                progress.cursor = Some(box_id);
                progress.scanned = progress.scanned.saturating_add(1);
            }
            if exhausted {
                progress.phase = READY;
            }
        }
        save(&txn, &progress)?;
        txn.commit()?;
        Reader::new(self)?.token_history_status()
    }
}
impl Reader {
    pub fn token_history_status(&self) -> Result<TokenHistoryStatus, StoreError> {
        let total_boxes = self.txn.open_table(BOXES)?.len()?;
        let current = anchor(
            &self.txn.open_table(META)?,
            &self.txn.open_table(HEADERS)?,
            total_boxes,
        )?;
        let progress = state(&self.txn.open_table(TOKEN_HISTORY_META)?)?
            .filter(|s| s.version == TOKEN_HISTORY_VERSION && Some(s.anchor) == current);
        let ready = progress.as_ref().is_some_and(|s| s.phase == READY);
        Ok(TokenHistoryStatus {
            version: TOKEN_HISTORY_VERSION,
            ready,
            phase: match progress.as_ref().map(|s| s.phase) {
                Some(CLEARING) => "clearing",
                Some(BUILDING) => "building",
                Some(READY) => "ready",
                _ => "stale",
            },
            scanned_boxes: progress.map_or(0, |s| s.scanned),
            total_boxes,
            indexed_memberships: if ready {
                self.txn.open_table(TOKEN_TXS)?.len()?
            } else {
                0
            },
        })
    }
}
