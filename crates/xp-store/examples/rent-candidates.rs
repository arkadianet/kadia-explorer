//! Offline phase-1 scanner. See docs/rent-candidates.md for the stream contract.
#[cfg(test)]
use redb::Database;
use redb::ReadableTable;
use serde_json::json;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use xp_store::{rows::BoxRow, tables::*};
use xp_types::rent::RENT_PERIOD;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Key = [u8; 36];
const RUN: usize = 4096;

#[path = "support/read_only.rs"]
mod read_only;
use read_only::open_source;

fn line(out: &mut impl Write, value: serde_json::Value) -> Result<()> {
    serde_json::to_writer(&mut *out, &value)?;
    writeln!(out)?;
    Ok(())
}

fn number(meta: &redb::ReadOnlyTable<&[u8], &[u8]>, key: &[u8]) -> Result<Option<u32>> {
    meta.get(key)?
        .map(|v| Ok(u32::from_be_bytes(v.value().try_into()?)))
        .transpose()
}

struct Progress {
    start: Instant,
    last: Instant,
    rows: u64,
    bytes: u64,
    candidates: u64,
}
impl Progress {
    fn report(&mut self, stage: &str, force: bool) {
        if force || self.last.elapsed() >= Duration::from_secs(5) {
            eprintln!(
                "stage={stage} rows={} row_bytes={} candidate_spends={} elapsed_seconds={:.1}",
                self.rows,
                self.bytes,
                self.candidates,
                self.start.elapsed().as_secs_f64()
            );
            self.last = Instant::now();
        }
    }
}

fn read_key(reader: &mut impl Read) -> Result<Option<Key>> {
    let mut key = [0; 36];
    if reader.read(&mut key[..1])? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut key[1..])?;
    Ok(Some(key))
}

// Fixed-size sorted runs, then two-way merge passes. No list of runs is retained.
// reverse=true changes txid|height into height|txid for the second sort.
fn sorted(mut input: File, dir: &Path, reverse: bool, p: &mut Progress) -> Result<File> {
    input.rewind()?;
    let mut reader = BufReader::new(input);
    let mut runs = tempfile::tempfile_in(dir)?;
    let mut count = 0u64;
    loop {
        let mut batch = Vec::with_capacity(RUN);
        while batch.len() < RUN {
            let Some(mut key) = read_key(&mut reader)? else {
                break;
            };
            if reverse {
                key.rotate_right(4);
            }
            batch.push(key);
        }
        if batch.is_empty() {
            break;
        }
        batch.sort_unstable();
        count += batch.len() as u64;
        for key in batch {
            runs.write_all(&key)?;
        }
        p.report("sort", false);
    }
    let mut width = RUN as u64;
    while width < count {
        let mut output = tempfile::tempfile_in(dir)?;
        let mut base = 0;
        while base < count {
            // Independent descriptors: try_clone shares the seek offset on Unix.
            // Positional readers below borrow the same file without a shared cursor.
            let left_end = (base + width).min(count);
            let right_end = (left_end + width).min(count);
            let (mut left, mut right) = (base, left_end);
            let mut a = None;
            let mut b = None;
            while left < left_end || right < right_end || a.is_some() || b.is_some() {
                if a.is_none() && left < left_end {
                    runs.seek(SeekFrom::Start(left * 36))?;
                    a = read_key(&mut runs)?;
                    left += 1;
                }
                if b.is_none() && right < right_end {
                    runs.seek(SeekFrom::Start(right * 36))?;
                    b = read_key(&mut runs)?;
                    right += 1;
                }
                let key = if b.is_none() || (a.is_some() && a <= b) {
                    a.take().ok_or("truncated sort run")?
                } else {
                    b.take().ok_or("truncated sort run")?
                };
                output.write_all(&key)?;
                p.report("merge", false);
            }
            base = right_end;
        }
        runs = output;
        width *= 2;
    }
    runs.rewind()?;
    Ok(runs)
}

fn scan(path: &Path, scratch: &Path, out: &mut impl Write) -> Result<()> {
    let mut p = Progress {
        start: Instant::now(),
        last: Instant::now(),
        rows: 0,
        bytes: 0,
        candidates: 0,
    };
    p.report("opening", true);
    let db = open_source(path)?;
    let snapshot = db.begin_read()?;
    let meta = snapshot.open_table(META)?;
    if number(&meta, META_SCHEMA)? != Some(SCHEMA_VERSION) {
        return Err("schema version mismatch".into());
    }
    let tip = number(&meta, META_INDEXED_HEIGHT)?;
    let partial = number(&meta, META_PARTIAL_FROM)?;
    let seeded = meta.get(META_GENESIS_SEEDED)?.is_some();
    if tip.is_some() && partial.is_none() && !seeded {
        return Err("indexed store lacks genesis provenance".into());
    }
    let anchor = tip
        .map(|height| -> Result<String> {
            let headers = snapshot.open_table(HEADERS)?;
            let row = headers
                .get(height.to_be_bytes().as_slice())?
                .ok_or("tip header missing")?;
            Ok(hex::encode(
                xp_store::rows::HeaderRow::decode(row.value())?.id,
            ))
        })
        .transpose()?;
    let coverage_complete = partial.is_none() && seeded;
    line(
        out,
        json!({"type":"start", "indexed_tip":tip, "indexed_tip_id":anchor,
        "partial_from":partial, "coverage_complete":false, "rent_period":RENT_PERIOD,
        "scan_height_range":tip.map(|h| [partial.unwrap_or(0), h])}),
    )?;
    out.flush()?;
    if !coverage_complete {
        eprintln!(
            "Coverage incomplete: partial or unseeded store; counts describe retained rows only."
        );
    }
    let mut keys = tempfile::tempfile_in(scratch)?;
    let boxes = snapshot.open_table(BOXES)?;
    for entry in boxes.iter()? {
        let (id, value) = entry?;
        p.rows += 1;
        p.bytes += (id.value().len() + value.value().len()) as u64;
        let row = BoxRow::decode(value.value())?;
        if let Some((tx, height)) = row.spent {
            if tip.is_none_or(|t| height > t) {
                return Err("spend above indexed tip".into());
            }
            if height
                .checked_sub(row.creation_height)
                .is_some_and(|age| age >= RENT_PERIOD)
            {
                p.candidates += 1;
                keys.write_all(&tx)?;
                keys.write_all(&height.to_be_bytes())?;
                line(
                    out,
                    json!({"type":"candidate_spend", "box_id":hex::encode(id.value()),
                    "spending_tx_id":hex::encode(tx), "spend_height":height}),
                )?;
            }
        }
        if p.last.elapsed() >= Duration::from_secs(5) {
            out.flush()?;
        }
        p.report("boxes", false);
    }
    out.flush()?;
    p.report("deduplicate", true);
    // Release the source before scratch sorting. Only one source snapshot/pass is used.
    drop(boxes);
    drop(meta);
    drop(snapshot);
    drop(db);
    let mut by_tx = sorted(keys, scratch, false, &mut p)?;
    let mut previous_tx = None;
    let mut transactions = 0u64;
    while let Some(key) = read_key(&mut by_tx)? {
        let tx: [u8; 32] = key[..32].try_into()?;
        if previous_tx != Some(tx) {
            transactions += 1;
            previous_tx = Some(tx);
        }
        p.report("count_transactions", false);
    }
    let mut by_height = sorted(by_tx, scratch, true, &mut p)?;
    let mut previous = None;
    let mut last_height = None;
    let mut minimum = None;
    let mut heights = 0u64;
    while let Some(key) = read_key(&mut by_height)? {
        let height = u32::from_be_bytes(key[..4].try_into()?);
        if last_height != Some(height) {
            heights += 1;
            minimum.get_or_insert(height);
            last_height = Some(height);
        }
        if previous != Some(key) {
            line(
                out,
                json!({"type":"candidate_transaction", "spending_tx_id":hex::encode(&key[4..]), "spend_height":height}),
            )?;
            previous = Some(key);
        }
        p.report("emit_transactions", false);
    }
    line(
        out,
        json!({"type":"summary", "scan_complete":true, "coverage_complete":coverage_complete,
        "indexed_tip":tip, "indexed_tip_id":anchor, "partial_from":partial,
        "scan_height_range":tip.map(|h| [partial.unwrap_or(0), h]),
        "candidate_height_range":minimum.zip(last_height).map(|(a,b)| [a,b]),
        "rows_scanned":p.rows, "row_bytes_scanned":p.bytes, "candidate_spends":p.candidates,
        "distinct_spending_transactions":transactions, "distinct_heights":heights}),
    )?;
    out.flush()?;
    p.report("done", true);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: rent-candidates <quiesced-store-path> <scratch-directory>".into());
    }
    scan(
        Path::new(&args[0]),
        Path::new(&args[1]),
        &mut std::io::stdout().lock(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use xp_store::{rows::TxRow, Store};

    fn fixture(path: &Path, partial: bool) {
        let store = Store::open(path).unwrap();
        store
            .seed_header_only_for_tests(RENT_PERIOD + 100, [9; 32])
            .unwrap();
        drop(store);
        let db = Database::open(path).unwrap();
        let write = db.begin_write().unwrap();
        {
            let mut meta = write.open_table(META).unwrap();
            meta.insert(META_GENESIS_SEEDED, &[1u8][..]).unwrap();
            if partial {
                meta.insert(META_PARTIAL_FROM, &10u32.to_be_bytes()[..])
                    .unwrap();
            }
            let mut boxes = write.open_table(BOXES).unwrap();
            // Exactly mature, under, over, old unspent, young, declared older than
            // inclusion, and declared younger than inclusion. First/third share a tx.
            for (id, creation, spent) in [
                (1, 100, Some(([1; 32], RENT_PERIOD + 100))),
                (2, 101, Some(([2; 32], RENT_PERIOD + 100))),
                (3, 99, Some(([1; 32], RENT_PERIOD + 100))),
                (4, 0, None),
                (5, 99, Some(([5; 32], 100))),
                (6, 0, Some(([6; 32], RENT_PERIOD))),
                (7, 200, Some(([7; 32], RENT_PERIOD + 100))),
                (8, u32::MAX, Some(([8; 32], 0))),
                (9, 100, Some(([9; 32], RENT_PERIOD + 100))),
            ] {
                let row = BoxRow {
                    gidx: id,
                    value: 1,
                    tree_hash: [0; 32],
                    creation_height: creation,
                    tx_id: [id as u8 + 20; 32],
                    index: 0,
                    size: 0,
                    tokens: vec![],
                    registers_json: "{}".into(),
                    spent,
                };
                boxes
                    .insert(&[id as u8; 32][..], row.encode().as_slice())
                    .unwrap();
            }
            let mut txs = write.open_table(TXS).unwrap();
            for (id, inclusion) in [(26, 100), (27, 0)] {
                let row = TxRow {
                    height: inclusion,
                    index: 0,
                    gidx: 0,
                    first_out_gidx: 0,
                    timestamp: 0,
                    size: 0,
                    fee: 0,
                    inputs: vec![],
                    data_inputs: vec![],
                    output_count: 1,
                };
                txs.insert(&[id; 32][..], row.encode().as_slice()).unwrap();
            }
        }
        write.commit().unwrap();
    }

    fn output(path: &Path, scratch: &Path) -> Vec<serde_json::Value> {
        let mut bytes = Vec::new();
        scan(path, scratch, &mut bytes).unwrap();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn boundaries_and_declared_creation_not_transaction_inclusion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.redb");
        fixture(&path, false);
        let rows = output(&path, dir.path());
        let ids: Vec<_> = rows
            .iter()
            .filter(|r| r["type"] == "candidate_spend")
            .map(|r| r["box_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            ids,
            vec![
                hex::encode([1; 32]),
                hex::encode([3; 32]),
                hex::encode([6; 32]),
                hex::encode([9; 32])
            ]
        );
        // Box 6 would be too young using tx 26's inclusion=100. Box 7 would
        // qualify using tx 27's inclusion=0. Both directions are pinned above.
        let summary = rows.last().unwrap();
        assert_eq!(summary["candidate_spends"], 4);
        assert_eq!(summary["distinct_spending_transactions"], 3);
        assert_eq!(summary["distinct_heights"], 2);
        assert_eq!(summary["rows_scanned"], 9);
        assert_eq!(
            summary["candidate_height_range"],
            json!([RENT_PERIOD, RENT_PERIOD + 100])
        );
        assert_eq!(summary["indexed_tip"], RENT_PERIOD + 100);
        assert_eq!(summary["coverage_complete"], true);
        assert_eq!(rows, output(&path, dir.path()));
    }

    #[test]
    fn source_descriptor_is_read_only_and_store_bytes_never_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.redb");
        fixture(&path, false);
        let before = std::fs::read(&path).unwrap();
        let db = open_source(&path).unwrap();
        // Exercise the actual scanner opener: even an accidental write transaction
        // cannot commit through the OS read-only descriptor (permissions alone
        // would not establish this when tests run as root).
        if let Ok(write) = db.begin_write() {
            write
                .open_table(META)
                .unwrap()
                .insert(b"attempt".as_slice(), b"write".as_slice())
                .unwrap();
            assert!(write.commit().is_err());
        }
        drop(db);
        output(&path, dir.path());
        assert_eq!(before, std::fs::read(&path).unwrap());
        let absent = dir.path().join("absent.redb");
        assert!(open_source(&absent).is_err());
        assert!(!absent.exists());
    }

    #[test]
    fn empty_and_partial_never_claim_complete_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.redb");
        drop(Store::open(&path).unwrap());
        let rows = output(&path, dir.path());
        let summary = rows.last().unwrap();
        for field in [
            "candidate_spends",
            "distinct_spending_transactions",
            "distinct_heights",
            "rows_scanned",
        ] {
            assert_eq!(summary[field], 0);
        }
        assert!(summary["indexed_tip"].is_null());
        assert!(summary["candidate_height_range"].is_null());
        assert_eq!(summary["coverage_complete"], false);
        let path = dir.path().join("partial.redb");
        fixture(&path, true);
        let rows = output(&path, dir.path());
        assert_eq!(rows.last().unwrap()["partial_from"], 10);
        assert_eq!(rows.last().unwrap()["coverage_complete"], false);
        assert_eq!(rows.last().unwrap()["scan_complete"], true);
    }

    #[test]
    fn external_merge_crosses_runs_and_preserves_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let mut input = tempfile::tempfile_in(dir.path()).unwrap();
        let mut expected = Vec::new();
        for i in (0..RUN * 3 + 17).rev() {
            let mut key = [0; 36];
            key[..8].copy_from_slice(&(i as u64 / 2).to_be_bytes());
            input.write_all(&key).unwrap();
            expected.push(key);
        }
        expected.sort_unstable();
        let mut p = Progress {
            start: Instant::now(),
            last: Instant::now(),
            rows: 0,
            bytes: 0,
            candidates: 0,
        };
        let mut sorted = sorted(input, dir.path(), false, &mut p).unwrap();
        for key in expected {
            assert_eq!(read_key(&mut sorted).unwrap(), Some(key));
        }
        assert!(read_key(&mut sorted).unwrap().is_none());
    }

    #[test]
    fn interrupted_output_has_no_completion_record_and_restart_is_safe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.redb");
        fixture(&path, false);
        struct Fails;
        impl Write for Fails {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(scan(&path, dir.path(), &mut Fails).is_err());
        assert_eq!(
            output(&path, dir.path()).last().unwrap()["scan_complete"],
            true
        );
    }
}
