pub mod apply;
pub mod keys;
pub mod read;
pub mod rows;
pub mod tables;

pub use read::Reader;

use redb::{Database, ReadableTable};
use std::path::Path;
use tables::*;
use xp_types::Hash32;

/// Number of trailing block heights for which an [`rows::UndoRow`] is retained, bounding how
/// deep a chain fork can be rolled back before a full reindex is required.
pub const ROLLBACK_WINDOW: u32 = 1_000;

/// Decodes a big-endian `u32` from an exactly-4-byte slice (a stored meta counter or a key's
/// height component), failing with [`StoreError::Corrupt`] instead of panicking on a
/// wrong-width value.
pub(crate) fn meta_u32(b: &[u8]) -> Result<u32, StoreError> {
    Ok(u32::from_be_bytes(
        b.try_into()
            .map_err(|_| StoreError::Corrupt("bad meta width"))?,
    ))
}

/// Decodes a big-endian `u64` from an exactly-8-byte slice (a stored meta counter), failing
/// with [`StoreError::Corrupt`] instead of panicking on a wrong-width value.
pub(crate) fn meta_u64(b: &[u8]) -> Result<u64, StoreError> {
    Ok(u64::from_be_bytes(
        b.try_into()
            .map_err(|_| StoreError::Corrupt("bad meta width"))?,
    ))
}

pub struct Store {
    db: Database,
}

impl Store {
    /// Opens (creating if absent) the redb database at `path`, ensuring every table in
    /// [`tables::ALL`] exists. On a fresh database the current [`tables::SCHEMA_VERSION`] is
    /// recorded; on an existing one a mismatched version is refused.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        let db = Database::create(path)?;
        let txn = db.begin_write()?;
        for t in ALL {
            txn.open_table(t)?;
        }
        {
            let mut meta = txn.open_table(META)?;
            let existing = meta
                .get(META_SCHEMA)?
                .map(|v| meta_u32(v.value()))
                .transpose()?;
            match existing {
                None => {
                    meta.insert(META_SCHEMA, keys::k_u32(SCHEMA_VERSION).as_slice())?;
                }
                Some(v) if v == SCHEMA_VERSION => {}
                Some(_) => return Err(StoreError::Corrupt("schema version mismatch")),
            }
        }
        txn.commit()?;
        Ok(Store { db })
    }

    /// Height of the last block applied, or `None` for an empty store.
    pub fn indexed_height(&self) -> Result<Option<u32>, StoreError> {
        let txn = self.db.begin_read()?;
        let meta = txn.open_table(META)?;
        meta.get(META_INDEXED_HEIGHT)?
            .map(|v| meta_u32(v.value()))
            .transpose()
    }

    pub fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, StoreError> {
        let txn = self.db.begin_read()?;
        let headers = txn.open_table(HEADERS)?;
        match headers.get(keys::k_u32(height).as_slice())? {
            Some(v) => Ok(Some(rows::HeaderRow::decode(v.value())?.id)),
            None => Ok(None),
        }
    }

    pub fn begin_read(&self) -> Result<redb::ReadTransaction, StoreError> {
        Ok(self.db.begin_read()?)
    }

    /// Seeds an empty store with a synthetic tip header at `height` whose id is `id`, so tests
    /// can exercise `apply_batch`'s contiguous-height / parent-id checks against a chosen
    /// starting point without replaying real history. Also marks the store as partial (see
    /// [`tables::META_PARTIAL_FROM`]), so `apply_batch` tolerates the first applied block's
    /// inputs being boxes older than the seed point — exactly the tests' situation, since no
    /// box data is seeded. Use [`Store::seed_header_only_for_tests`] to seed without that
    /// tolerance.
    #[doc(hidden)]
    pub fn seed_for_tests(&self, height: u32, id: Hash32) -> Result<(), StoreError> {
        self.seed_impl(height, id, true)
    }

    /// Like [`Store::seed_for_tests`] but does NOT mark the store partial, so a missing input
    /// at the next applied height is treated as real corruption. For tests that need to
    /// exercise that corruption check without replaying from genesis.
    #[doc(hidden)]
    pub fn seed_header_only_for_tests(&self, height: u32, id: Hash32) -> Result<(), StoreError> {
        self.seed_impl(height, id, false)
    }

    fn seed_impl(&self, height: u32, id: Hash32, partial: bool) -> Result<(), StoreError> {
        let txn = self.db.begin_write()?;
        {
            let hrow = rows::HeaderRow {
                id,
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
            txn.open_table(HEADERS)?
                .insert(keys::k_u32(height).as_slice(), hrow.encode().as_slice())?;
            txn.open_table(HEADER_BY_ID)?
                .insert(id.as_slice(), keys::k_u32(height).as_slice())?;
            let mut meta = txn.open_table(META)?;
            meta.insert(META_INDEXED_HEIGHT, keys::k_u32(height).as_slice())?;
            if partial {
                meta.insert(META_PARTIAL_FROM, keys::k_u32(height).as_slice())?;
            }
        }
        txn.commit()?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redb: {0}")]
    Redb(String),
    #[error("corrupt row: {0}")]
    Corrupt(&'static str),
    #[error("fork deeper than rollback window ({0} blocks): reindex required")]
    ReindexRequired(u32),
    #[error("parent mismatch at height {height}: have {have}, block says {want}")]
    ParentMismatch {
        height: u32,
        have: String,
        want: String,
    },
    #[error("wire: {0}")]
    Wire(#[from] xp_wire::WireError),
}

macro_rules! from_redb_error {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for StoreError {
                fn from(e: $ty) -> Self {
                    StoreError::Redb(e.to_string())
                }
            }
        )*
    };
}

from_redb_error!(
    redb::Error,
    redb::TableError,
    redb::StorageError,
    redb::TransactionError,
    redb::CommitError,
    redb::DatabaseError,
);
