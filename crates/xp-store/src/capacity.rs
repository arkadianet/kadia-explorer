//! redb 2.6.3 len() reads root.length; no scan or separately persisted counter.
use redb::{ReadableTableMetadata, WriteTransaction};
use xp_wire::DecodedBox;

use crate::{extras::register_count, tables::REGISTER_IDX, Store, StoreError};

/// Enforcement is opt-in after the operator measures capacity.
pub const DEFAULT_REGISTER_INDEX_CEILING: Option<u64> = None;

fn check(existing: u64, additional: u64, ceiling: Option<u64>) -> Result<(), StoreError> {
    let Some(ceiling) = ceiling else {
        return Ok(());
    };
    let total = existing
        .checked_add(additional)
        .ok_or(StoreError::RegisterCountOverflow)?;
    if total >= ceiling {
        return Err(StoreError::RegisterCapacity {
            existing,
            additional,
            ceiling,
        });
    }
    Ok(())
}

impl Store {
    pub fn register_index_ceiling(&self) -> Option<u64> {
        self.register_index_ceiling
    }

    /// Cheap committed occupancy, in entries. Includes spent boxes and genesis.
    pub fn register_index_entries(&self) -> Result<u64, StoreError> {
        Ok(self.begin_read()?.open_table(REGISTER_IDX)?.len()?)
    }

    pub(crate) fn check_register_capacity<'a>(
        &self,
        txn: &WriteTransaction,
        mut outputs: impl Iterator<Item = &'a DecodedBox>,
    ) -> Result<(), StoreError> {
        if self.register_index_ceiling.is_none() {
            return Ok(());
        }
        // Valid outputs receive distinct monotonically allocated gidx keys, even when
        // register values repeat. Use exactly Extras' presence rules, never value dedupe.
        let additional = outputs.try_fold(0u64, |n, o| {
            n.checked_add(register_count(o))
                .ok_or(StoreError::RegisterCountOverflow)
        })?;
        let existing = txn.open_table(REGISTER_IDX)?.len()?;
        check(existing, additional, self.register_index_ceiling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unset_never_rejects_regardless_of_occupancy() {
        for existing in [0, 100_000_000, 100_000_001, u64::MAX] {
            for additional in [0, 1, u64::MAX] {
                assert!(check(existing, additional, None).is_ok());
            }
            assert!(matches!(
                check(existing, 0, Some(existing)),
                Err(StoreError::RegisterCapacity { .. })
            ));
        }
    }

    #[test]
    fn count_overflow_is_local_and_never_wraps() {
        assert!(matches!(
            check(u64::MAX, 1, Some(u64::MAX)),
            Err(StoreError::RegisterCountOverflow)
        ));
        assert!(check(u64::MAX - 2, 1, Some(u64::MAX)).is_ok());
        assert!(matches!(
            check(u64::MAX - 1, 1, Some(u64::MAX)),
            Err(StoreError::RegisterCapacity { .. })
        ));
    }
}
