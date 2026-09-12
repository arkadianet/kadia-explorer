//! Test-only logical snapshots. No database write hook is needed: mutation tests close
//! Store before opening redb directly, so there is always exactly one owner.
use redb::{ReadableTable, TableHandle};
use std::collections::BTreeMap;
use xp_store::{rows::UndoRow, tables::*, Store};

pub type Rows = BTreeMap<Vec<u8>, Vec<u8>>;
pub type Snapshot = BTreeMap<String, Rows>;

pub fn snapshot(store: &Store) -> Snapshot {
    let txn = store.begin_read().unwrap();
    ALL.into_iter()
        .map(|def| {
            let rows = txn
                .open_table(def)
                .unwrap()
                .iter()
                .unwrap()
                .map(|entry| {
                    let (k, v) = entry.unwrap();
                    (k.value().to_vec(), v.value().to_vec())
                })
                .collect();
            (def.name().to_owned(), rows)
        })
        .collect()
}

pub fn difference(actual: &Snapshot, expected: &Snapshot) -> Result<(), String> {
    for def in ALL {
        let name = def.name();
        let a = &actual[name];
        let e = &expected[name];
        for key in a.keys().chain(e.keys()) {
            let av = a.get(key);
            let ev = e.get(key);
            // Only this vector is emitted in randomized HashMap order. Keep every
            // entry (including duplicates), value, and all other ordering significant.
            let normalize = |bytes: &Vec<u8>| -> Result<Vec<u8>, String> {
                if name != UNDO.name() {
                    return Ok(bytes.clone());
                }
                let mut row = UndoRow::decode(bytes).map_err(|e| e.to_string())?;
                // Reject trailing bytes / noncanonical representations before sorting.
                if row.encode() != *bytes {
                    return Err("noncanonical undo bytes".into());
                }
                row.prev_balances.sort_by_key(|(tree, _)| *tree);
                Ok(row.encode())
            };
            let av = av
                .map(normalize)
                .transpose()
                .map_err(|e| format!("{name}: {e}"))?;
            let ev = ev
                .map(normalize)
                .transpose()
                .map_err(|e| format!("expected {name}: {e}"))?;
            if av != ev {
                return Err(format!(
                    "table={name} key={} actual={} expected={}",
                    hex::encode(key),
                    av.map(hex::encode).unwrap_or_else(|| "MISSING".into()),
                    ev.map(hex::encode).unwrap_or_else(|| "MISSING".into())
                ));
            }
        }
    }
    Ok(())
}
