pub mod keys;
pub mod rows;
pub mod tables;

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
