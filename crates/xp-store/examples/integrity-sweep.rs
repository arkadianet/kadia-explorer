//! Offline required-reference audit. Contract and coverage: docs/integrity-sweep.md.
use redb::{ReadableTable, ReadableTableMetadata, TableHandle};
use serde_json::json;
use std::{
    collections::VecDeque,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
use xp_store::{rows::*, tables::*};
#[path = "support/read_only.rs"]
mod read_only;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn line(out: &mut impl Write, value: serde_json::Value) -> Result<()> {
    serde_json::to_writer(&mut *out, &value)?;
    writeln!(out)?;
    Ok(())
}
// These are exactly the tables traversed by walk!, in traversal order. Lookup-only
// tables are deliberately excluded. len() reads redb's stored entry count.
const WALK_TABLES: [Tbl; 23] = [
    HEADERS,
    HEADER_BY_ID,
    TX_BY_GIDX,
    TREE_TXS,
    TXS,
    BOX_BY_GIDX,
    TREE_BOXES,
    TREE_UNSPENT,
    TEMPLATE_BOXES,
    TEMPLATE_UNSPENT,
    TOKEN_BOXES,
    TOKEN_UNSPENT,
    REGISTER_IDX,
    RENT_MATURES,
    BOXES,
    ERGO_TREES,
    TREE_BALANCE,
    TOKENS_BY_GIDX,
    TOKENS_BY_HOLDERS,
    TEMPLATES,
    RICH,
    TOKEN_HOLDERS,
    UNDO,
];

// This interface cannot iterate entries or decode values.
fn row_count(table: &impl ReadableTableMetadata) -> redb::Result<u64> {
    table.len()
}

struct StageProgress {
    name: String,
    completed: u64,
    total: u64,
    samples: VecDeque<(Instant, u64)>,
}
impl StageProgress {
    fn new(name: &str, total: u64) -> Self {
        Self {
            name: name.into(),
            completed: 0,
            total,
            samples: VecDeque::from([(Instant::now(), 0)]),
        }
    }
    fn eta(&mut self, now: Instant) -> String {
        // At most thirteen samples spanning sixty seconds; reset at each stage.
        while self.samples.len() > 1
            && now.duration_since(self.samples[0].0) > Duration::from_secs(60)
        {
            self.samples.pop_front();
        }
        let (start, completed) = self.samples[0];
        let elapsed = now.duration_since(start).as_secs_f64();
        let advanced = self.completed - completed;
        let eta = if self.completed == self.total {
            "0".into()
        } else if elapsed < 15.0 {
            "unavailable(warming_up)".into()
        } else if advanced == 0 {
            "unavailable(no_recent_progress)".into()
        } else if elapsed > 60.0 {
            // A single slow operation may prevent sampling for an entire window.
            "unavailable(stale_window)".into()
        } else {
            format!(
                "{:.0}",
                (self.total - self.completed) as f64 * elapsed / advanced as f64
            )
        };
        if now.duration_since(self.samples.back().unwrap().0) >= Duration::from_secs(5) {
            self.samples.push_back((now, self.completed));
        }
        eta
    }
}

struct Audit<'a, W> {
    out: &'a mut W,
    counts: [u64; 28],
    checks: [u64; 28],
    partial_gaps: [u64; 2],
    rows: u64,
    total_rows: u64,
    stage: StageProgress,
    last: Instant,
}
impl<W: Write> Audit<'_, W> {
    fn check(
        &mut self,
        r: usize,
        ok: bool,
        source: &str,
        key: &[u8],
        reference: &str,
    ) -> Result<()> {
        self.checks[r - 1] += 1;
        if !ok {
            self.counts[r - 1] += 1;
            line(
                self.out,
                json!({"type":"finding","invariant":format!("R{r}"),"source":source,"key":hex::encode(key),"reference":reference}),
            )?;
        }
        self.progress(false)
    }
    fn progress(&mut self, force: bool) -> Result<()> {
        if force || self.last.elapsed() >= Duration::from_secs(5) {
            let eta = self.stage.eta(Instant::now());
            eprintln!(
                "stage={} completed={}/{} rows={}/{} checks={} findings={} stage_eta_s={} overall_eta_s=unavailable(mixed_stage_costs)",
                self.stage.name,
                self.stage.completed,
                self.stage.total,
                self.rows,
                self.total_rows,
                self.checks.iter().sum::<u64>(),
                self.counts.iter().sum::<u64>(),
                eta
            );
            self.out.flush()?;
            self.last = Instant::now();
        }
        Ok(())
    }
    fn stage(&mut self, name: &str, total: u64) -> Result<()> {
        self.stage = StageProgress::new(name, total);
        self.progress(true)
    }
    fn completed(&mut self) -> Result<()> {
        self.stage.completed += 1;
        self.progress(false)
    }
    fn required(
        &mut self,
        snap: &redb::ReadTransaction,
        r: usize,
        source: &str,
        key: &[u8],
        target: Tbl,
        id: &[u8],
    ) -> Result<bool> {
        let exists = snap.open_table(target)?.get(id)?.is_some();
        self.check(
            r,
            exists,
            source,
            key,
            &format!("{}:{}", target.name(), hex::encode(id)),
        )?;
        Ok(exists)
    }
    fn box_ref(
        &mut self,
        snap: &redb::ReadTransaction,
        r: usize,
        source: &str,
        key: &[u8],
        g: u64,
    ) -> Result<()> {
        let index = snap.open_table(BOX_BY_GIDX)?;
        if let Some(id) = index.get(g.to_be_bytes().as_slice())? {
            if self.required(snap, r, source, key, BOXES, id.value())? {
                let row =
                    BoxRow::decode(snap.open_table(BOXES)?.get(id.value())?.unwrap().value())?;
                self.check(r, row.gidx == g, source, key, "box gidx agrees")?;
            }
        } else {
            self.check(r, false, source, key, &format!("box_by_gidx:{g}"))?;
        }
        Ok(())
    }
    fn assets(
        &mut self,
        snap: &redb::ReadTransaction,
        partial: bool,
        source: &str,
        key: &[u8],
        assets: &[(xp_types::Hash32, u64)],
    ) -> Result<()> {
        let tokens = snap.open_table(TOKENS)?;
        for (id, _) in assets {
            let ok = tokens.get(id.as_slice())?.is_some();
            if partial && !ok {
                self.partial_gaps[1] += 1;
            } else {
                for r in [11, 27] {
                    self.check(r, ok, source, key, &format!("tokens:{}", hex::encode(id)))?;
                }
            }
        }
        Ok(())
    }
}
fn number<const N: usize>(
    meta: &redb::ReadOnlyTable<&[u8], &[u8]>,
    key: &[u8],
) -> Result<Option<[u8; N]>> {
    meta.get(key)?
        .map(|v| Ok(v.value().try_into()?))
        .transpose()
}
fn scan(path: &Path, out: &mut impl Write) -> Result<u8> {
    eprintln!("Opening quiesced store read-only");
    let db = read_only::open_source(path)?;
    let snap = db.begin_read()?;
    // Missing tables/invalid encodings are operational incompletion, never a clean scan.
    for table in ALL {
        snap.open_table(table)?;
    }
    let meta = snap.open_table(META)?;
    if number(&meta, META_SCHEMA)?.map(u32::from_be_bytes) != Some(SCHEMA_VERSION) {
        return Err("schema version mismatch".into());
    }
    let tip = number(&meta, META_INDEXED_HEIGHT)?.map(u32::from_be_bytes);
    let partial = number(&meta, META_PARTIAL_FROM)?.map(u32::from_be_bytes);
    let seeded = meta.get(META_GENESIS_SEEDED)?.is_some();
    let anchor = tip
        .map(|h| -> Result<_> {
            Ok(snap
                .open_table(HEADERS)?
                .get(h.to_be_bytes().as_slice())?
                .map(|v| HeaderRow::decode(v.value()).map(|h| hex::encode(h.id)))
                .transpose()?)
        })
        .transpose()?
        .flatten();
    let mut recognized = seeded && partial.is_none();
    for (id, value) in xp_store::read::MAINNET_GENESIS {
        let id = hex::decode(id)?;
        recognized &= snap
            .open_table(BOXES)?
            .get(id.as_slice())?
            .map(|v| {
                BoxRow::decode(v.value())
                    .map(|b| b.value == value && b.tx_id == [0; 32] && b.creation_height == 0)
            })
            .transpose()?
            .unwrap_or(false);
    }
    let coverage = json!({"R1":"retained height interval","R2":"reverse headers","R3":"global and tree tx indexes","R4":"global allocation and header tx ranges","R5":"tx output ranges","R6":"all retained box membership indexes","R7":"box trees","R8":"retained tx inputs; P1 counted separately","R9":"register JSON","R10":"both token listings","R11":"box and balance assets; P2 counted separately","R12":"tree balances","R13":"not independently reachable: same immutable snapshot reload","R14":"rich trees/balances/amounts","R15":"holder trees/amounts","R16":"template example trees","R17":"rent box resolution and live state","R18":"recognized mainnet supply only; O5 recognition limit","R19":"history anchors, box gidx/creator, known tree balance; zero creator policy only on recognized mainnet","R20":"tip header","R21":"retained inputs via R8; future supplied blocks not reachable","R22":"retained box trees","R23":"retained undo created boxes/txs/headers","R24":"retained undo spent boxes; same-block creations still exist in quiesced state","R25":"retained undo new/previous tokens","R26":"retained undo touched balances","R27":"retained asset references; future transitions not reachable","R28":"initialized allocation counters"});
    line(
        out,
        json!({"type":"start","indexed_tip":tip,"indexed_tip_id":anchor,"partial_from":partial,"partial":partial.is_some(),"coverage_complete":seeded && partial.is_none(),"mainnet_recognized":recognized,"coverage":coverage}),
    )?;
    out.flush()?;
    let table_counts: Vec<_> = WALK_TABLES
        .iter()
        .map(|&table| {
            Ok((
                table.name().to_owned(),
                row_count(&snap.open_table(table)?)?,
            ))
        })
        .collect::<Result<_>>()?;
    let total_rows = table_counts.iter().map(|(_, count)| count).sum();
    let mut a = Audit {
        out,
        counts: [0; 28],
        checks: [0; 28],
        partial_gaps: [0; 2],
        rows: 0,
        total_rows,
        stage: StageProgress::new("metadata", 1),
        last: Instant::now(),
    };
    a.progress(true)?;
    for (key, needed) in [
        (META_NEXT_BOX_GIDX, tip.is_some() || seeded),
        (META_NEXT_TX_GIDX, tip.is_some()),
    ] {
        if needed {
            a.check(
                28,
                number::<8>(&meta, key)?.is_some(),
                "meta",
                key,
                "allocation counter",
            )?;
        }
    }
    if let Some(tip) = tip {
        a.required(
            &snap,
            20,
            "meta",
            META_INDEXED_HEIGHT,
            HEADERS,
            &tip.to_be_bytes(),
        )?;
        a.completed()?;
        a.progress(true)?;
        a.stage(
            "retained_height",
            (u64::from(tip) + 1).saturating_sub(u64::from(partial.unwrap_or(1))),
        )?;
        for h in partial.unwrap_or(1)..=tip {
            for r in [1, 19] {
                a.required(
                    &snap,
                    r,
                    "retained_height",
                    &h.to_be_bytes(),
                    HEADERS,
                    &h.to_be_bytes(),
                )?;
            }
            a.completed()?;
        }
    } else {
        a.completed()?;
        a.progress(true)?;
        a.stage("retained_height", 0)?;
    }
    a.progress(true)?;
    if let Some(next) = number(&meta, META_NEXT_TX_GIDX)?.map(u64::from_be_bytes) {
        a.stage("allocated_tx", next)?;
        for g in 0..next {
            a.required(
                &snap,
                4,
                "allocated_tx",
                &g.to_be_bytes(),
                TX_BY_GIDX,
                &g.to_be_bytes(),
            )?;
            a.completed()?;
        }
    } else {
        a.stage("allocated_tx", 0)?;
    }
    a.progress(true)?;
    macro_rules! walk { ($table:expr, $k:ident, $v:ident, $body:block) => {{
        a.stage($table.name(), table_counts.iter().find(|(name, _)| *name == $table.name()).unwrap().1)?;
        let table = snap.open_table($table)?;
        for entry in table.iter()? { let (key,value)=entry?; let $k=key.value(); let $v=value.value(); a.rows+=1; $body a.completed()?; }
        a.progress(true)?;
    }}; }
    walk!(HEADERS, k, v, {
        let row = HeaderRow::decode(v)?;
        let end = row
            .first_tx_gidx
            .checked_add(row.tx_count as u64)
            .ok_or("tx range overflow")?;
        for g in row.first_tx_gidx..end {
            if a.required(&snap, 4, "headers", k, TX_BY_GIDX, &g.to_be_bytes())? {
                let indexes = snap.open_table(TX_BY_GIDX)?;
                let id = indexes.get(g.to_be_bytes().as_slice())?.unwrap();
                if let Some(v) = snap.open_table(TXS)?.get(id.value())? {
                    let tx = TxRow::decode(v.value())?;
                    a.check(
                        4,
                        tx.height.to_be_bytes() == k
                            && u64::from(tx.index) == g - row.first_tx_gidx
                            && tx.gidx == g,
                        "headers",
                        k,
                        "block tx range agrees",
                    )?;
                }
            }
        }
    });
    walk!(HEADER_BY_ID, k, v, {
        if a.required(&snap, 2, "header_by_id", k, HEADERS, v)? {
            a.check(
                2,
                HeaderRow::decode(snap.open_table(HEADERS)?.get(v)?.unwrap().value())?.id == k,
                "header_by_id",
                k,
                "header id agrees",
            )?;
        }
    });
    walk!(TX_BY_GIDX, k, v, {
        if a.required(&snap, 3, "tx_by_gidx", k, TXS, v)? {
            a.check(
                4,
                TxRow::decode(snap.open_table(TXS)?.get(v)?.unwrap().value())?
                    .gidx
                    .to_be_bytes()
                    == k,
                "tx_by_gidx",
                k,
                "tx gidx agrees",
            )?;
        }
    });
    walk!(TREE_TXS, k, _v, {
        let g = xp_store::keys::gidx_of_composite(k)?;
        if let Some(id) = snap
            .open_table(TX_BY_GIDX)?
            .get(g.to_be_bytes().as_slice())?
        {
            a.required(&snap, 3, "tree_txs", k, TXS, id.value())?;
        } else {
            a.check(3, false, "tree_txs", k, "tx_by_gidx")?;
        }
    });
    walk!(TXS, k, v, {
        let row = TxRow::decode(v)?;
        let end = row
            .first_out_gidx
            .checked_add(row.output_count as u64)
            .ok_or("output range overflow")?;
        for g in row.first_out_gidx..end {
            a.box_ref(&snap, 5, "txs", k, g)?;
            if let Some(id) = snap
                .open_table(BOX_BY_GIDX)?
                .get(g.to_be_bytes().as_slice())?
            {
                if let Some(v) = snap.open_table(BOXES)?.get(id.value())? {
                    a.check(
                        5,
                        BoxRow::decode(v.value())?.tx_id == k,
                        "txs",
                        k,
                        "output belongs to transaction",
                    )?;
                }
            }
        }
        for id in row.inputs {
            let ok = snap.open_table(BOXES)?.get(id.as_slice())?.is_some();
            if partial.is_some() && !ok {
                a.partial_gaps[0] += 1;
            } else {
                for r in [8, 21] {
                    a.check(r, ok, "txs", k, &format!("boxes:{}", hex::encode(id)))?;
                }
            }
        }
    });
    walk!(BOX_BY_GIDX, k, v, {
        a.required(&snap, 6, "box_by_gidx", k, BOXES, v)?;
    });
    for index in [
        TREE_BOXES,
        TREE_UNSPENT,
        TEMPLATE_BOXES,
        TEMPLATE_UNSPENT,
        TOKEN_BOXES,
        TOKEN_UNSPENT,
        REGISTER_IDX,
        RENT_MATURES,
    ] {
        walk!(index, k, _v, {
            let g = xp_store::keys::gidx_of_composite(k)?;
            a.box_ref(&snap, 6, index.name(), k, g)?;
            if index.name() == TREE_BOXES.name() {
                a.box_ref(&snap, 19, index.name(), k, g)?;
                if let Some(id) = snap
                    .open_table(BOX_BY_GIDX)?
                    .get(g.to_be_bytes().as_slice())?
                {
                    if let Some(v) = snap.open_table(BOXES)?.get(id.value())? {
                        a.check(
                            19,
                            k.get(..32) == Some(BoxRow::decode(v.value())?.tree_hash.as_slice()),
                            index.name(),
                            k,
                            "historical tree agrees",
                        )?;
                    }
                }
            }
            if index.name() == RENT_MATURES.name() {
                a.box_ref(&snap, 17, index.name(), k, g)?;
                if let Some(id) = snap
                    .open_table(BOX_BY_GIDX)?
                    .get(g.to_be_bytes().as_slice())?
                {
                    if let Some(v) = snap.open_table(BOXES)?.get(id.value())? {
                        a.check(
                            17,
                            BoxRow::decode(v.value())?.spent.is_none(),
                            index.name(),
                            k,
                            "box must be live",
                        )?;
                    }
                }
            }
        });
    }
    walk!(BOXES, k, v, {
        let row = BoxRow::decode(v)?;
        for r in [7, 22] {
            a.required(&snap, r, "boxes", k, ERGO_TREES, &row.tree_hash)?;
        }
        a.check(
            9,
            serde_json::from_str::<serde_json::Value>(&row.registers_json).is_ok(),
            "boxes",
            k,
            "valid registers_json",
        )?;
        a.assets(&snap, partial.is_some(), "boxes", k, &row.tokens)?;
        a.box_ref(&snap, 19, "boxes", k, row.gidx)?;
        if let Some(id) = snap
            .open_table(BOX_BY_GIDX)?
            .get(row.gidx.to_be_bytes().as_slice())?
        {
            a.check(
                19,
                id.value() == k,
                "boxes",
                k,
                "historical box index agrees",
            )?;
        }
        if row.tx_id != [0; 32] {
            a.required(&snap, 19, "boxes", k, TXS, &row.tx_id)?;
        } else if recognized {
            a.check(
                19,
                xp_store::read::MAINNET_GENESIS
                    .iter()
                    .any(|(id, _)| *id == hex::encode(k)),
                "boxes",
                k,
                "known genesis zero creator",
            )?;
        }
    });
    walk!(ERGO_TREES, k, _v, {
        for r in [12, 19] {
            a.required(&snap, r, "ergo_trees", k, TREE_BALANCE, k)?;
        }
    });
    walk!(TREE_BALANCE, k, v, {
        a.assets(
            &snap,
            partial.is_some(),
            "tree_balance",
            k,
            &BalanceRow::decode(v)?.tokens,
        )?;
    });
    walk!(TOKENS_BY_GIDX, k, v, {
        a.required(&snap, 10, "tokens_by_gidx", k, TOKENS, v)?;
    });
    walk!(TOKENS_BY_HOLDERS, k, _v, {
        a.required(
            &snap,
            10,
            "tokens_by_holders",
            k,
            TOKENS,
            k.get(8..).ok_or("short token key")?,
        )?;
    });
    walk!(TEMPLATES, k, v, {
        a.required(
            &snap,
            16,
            "templates",
            k,
            ERGO_TREES,
            &TemplateRow::decode(v)?.example_tree,
        )?;
    });
    walk!(RICH, k, _v, {
        let tree = k.get(8..).ok_or("short rich key")?;
        a.required(&snap, 14, "rich", k, ERGO_TREES, tree)?;
        if a.required(&snap, 14, "rich", k, TREE_BALANCE, tree)? {
            let balance =
                BalanceRow::decode(snap.open_table(TREE_BALANCE)?.get(tree)?.unwrap().value())?;
            a.check(
                14,
                balance.nano.to_be_bytes() == k[..8],
                "rich",
                k,
                "balance amount agrees",
            )?;
        }
    });
    walk!(TOKEN_HOLDERS, k, _v, {
        if k.len() != 72 {
            return Err("invalid holder key".into());
        }
        let tree = &k[40..];
        let mut id = [0; 64];
        id[..32].copy_from_slice(&k[..32]);
        id[32..].copy_from_slice(tree);
        a.required(&snap, 15, "token_holders", k, ERGO_TREES, tree)?;
        if a.required(&snap, 15, "token_holders", k, TOKEN_HOLDER_AMT, &id)? {
            a.check(
                15,
                snap.open_table(TOKEN_HOLDER_AMT)?
                    .get(id.as_slice())?
                    .unwrap()
                    .value()
                    == &k[32..40],
                "token_holders",
                k,
                "holder amount agrees",
            )?;
        }
    });
    a.stage("emission", 1)?;
    if recognized {
        let id = hex::decode(xp_store::read::MAINNET_GENESIS[0].0)?;
        let row = BoxRow::decode(snap.open_table(BOXES)?.get(id.as_slice())?.unwrap().value())?;
        a.check(
            18,
            meta.get(META_EMISSION_TREE_HASH)?
                .is_some_and(|v| v.value() == row.tree_hash),
            "meta",
            META_EMISSION_TREE_HASH,
            "genesis emission tree",
        )?;
        if a.required(&snap, 18, "emission", &id, TREE_BALANCE, &row.tree_hash)? {
            let balance = BalanceRow::decode(
                snap.open_table(TREE_BALANCE)?
                    .get(row.tree_hash.as_slice())?
                    .unwrap()
                    .value(),
            )?;
            a.check(
                18,
                balance.nano <= xp_types::GENESIS_TOTAL_NANO,
                "emission",
                &id,
                "remaining <= genesis total",
            )?;
        }
    }
    a.completed()?;
    a.progress(true)?;
    walk!(UNDO, k, v, {
        let row = UndoRow::decode(v)?;
        a.required(&snap, 23, "undo", k, HEADERS, k)?;
        for id in row.created_boxes {
            a.required(&snap, 23, "undo", k, BOXES, &id)?;
        }
        for id in row.tx_ids {
            a.required(&snap, 23, "undo", k, TXS, &id)?;
        }
        // Same-block creations have not been un-created in this read-only snapshot.
        for id in row.spent_boxes {
            a.required(&snap, 24, "undo", k, BOXES, &id)?;
        }
        for id in row
            .new_tokens
            .into_iter()
            .chain(row.prev_tokens.into_iter().map(|(id, _)| id))
        {
            a.required(&snap, 25, "undo", k, TOKENS, &id)?;
        }
        for (tree, _) in row.prev_balances {
            a.required(&snap, 26, "undo", k, TREE_BALANCE, &tree)?;
        }
    });
    let total = a.counts.iter().sum::<u64>();
    let counts: serde_json::Map<String, serde_json::Value> = (1..=28)
        .map(|r| {
            (
                format!("R{r}"),
                json!({"checks":a.checks[r-1],"findings":a.counts[r-1]}),
            )
        })
        .collect();
    line(
        a.out,
        json!({"type":"summary","scan_complete":true,"verdict":if total==0 {"clean_within_scope"}else{"findings"},"findings":total,"per_invariant":counts,"rows_scanned":a.rows,"indexed_tip":tip,"indexed_tip_id":anchor,"partial":partial.is_some(),"partial_from":partial,"coverage_complete":seeded && partial.is_none(),"permitted_gaps":{"P1":a.partial_gaps[0],"P2":a.partial_gaps[1]},"amounts_complete":seeded && partial.is_none(),"coverage":coverage}),
    )?;
    a.progress(true)?;
    Ok(if total == 0 { 0 } else { 1 })
}
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut out = std::io::stdout().lock();
    let result = if args.len() == 1 {
        scan(Path::new(&args[0]), &mut out)
    } else {
        Err("usage: integrity-sweep <quiesced-store-path>".into())
    };
    match result {
        Ok(code) => code.into(),
        Err(error) => {
            eprintln!("integrity sweep could not complete: {error}");
            let _ = line(
                &mut out,
                json!({"type":"incomplete","scan_complete":false,"error":error.to_string()}),
            );
            2.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::io::{BufRead, Read};
    use std::process::{Command, Stdio};
    use xp_store::Store;

    #[test]
    fn denominator_uses_only_metadata_len() {
        struct MetadataOnly(std::cell::Cell<u32>);
        impl ReadableTableMetadata for MetadataOnly {
            fn stats(&self) -> redb::Result<redb::TableStats> {
                panic!("must not traverse pages for stats")
            }
            fn len(&self) -> redb::Result<u64> {
                self.0.set(self.0.get() + 1);
                Ok(81_680_000)
            }
        }
        let table = MetadataOnly(std::cell::Cell::new(0));
        assert_eq!(row_count(&table).unwrap(), 81_680_000);
        assert_eq!(table.0.get(), 1);
    }

    #[test]
    fn table_denominator_matches_completed_fixture_walks() {
        let dir = tempfile::tempdir().unwrap();
        for partial in [false, true] {
            let path = dir.path().join(format!("{partial}.redb"));
            fixture(&path, partial);
            let db = read_only::open_source(&path).unwrap();
            let snap = db.begin_read().unwrap();
            let total: u64 = WALK_TABLES
                .iter()
                .map(|&table| row_count(&snap.open_table(table).unwrap()).unwrap())
                .sum();
            drop(snap);
            drop(db);
            assert_eq!(output(&path).1.last().unwrap()["rows_scanned"], total);
        }
    }

    #[test]
    fn eta_uses_recent_stage_progress_and_handles_stalls() {
        let mut p = StageProgress::new("test", 1000);
        let start = p.samples[0].0;
        assert_eq!(p.eta(start), "unavailable(warming_up)");
        p.completed = 100;
        assert_eq!(p.eta(start + Duration::from_secs(20)), "180");
        p.completed = 200;
        assert_eq!(p.eta(start + Duration::from_secs(40)), "160");
        // Discard the fast start. Only 10 completed in the recent 50 seconds.
        p.completed = 210;
        assert_eq!(p.eta(start + Duration::from_secs(90)), "3950");
        assert_eq!(
            p.eta(start + Duration::from_secs(151)),
            "unavailable(no_recent_progress)"
        );
        p.completed = 1000;
        assert_eq!(p.eta(start + Duration::from_secs(156)), "0");
        let mut slow = StageProgress::new("slow", 10);
        slow.completed = 1;
        assert_eq!(
            slow.eta(slow.samples[0].0 + Duration::from_secs(120)),
            "unavailable(stale_window)"
        );
        let mut next = StageProgress::new("next", 10);
        assert_eq!(next.eta(next.samples[0].0), "unavailable(warming_up)");
        let mut empty = StageProgress::new("empty", 0);
        assert_eq!(empty.eta(empty.samples[0].0), "0");
        assert!(p.samples.len() <= 13);
    }

    fn fixture(path: &Path, partial: bool) {
        let store = Store::open(path).unwrap();
        if partial {
            let block = xp_wire::decode_block(
                &std::fs::read_to_string(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/fixtures/blocks/453051.json"
                ))
                .unwrap(),
            )
            .unwrap();
            store
                .seed_for_tests(453050, block.header.parent_id.0)
                .unwrap();
            store.apply_batch(&[block], true).unwrap();
        } else {
            let boxes = xp_wire::decode_genesis_boxes(
                &std::fs::read_to_string(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/fixtures/genesis.json"
                ))
                .unwrap(),
            )
            .unwrap();
            store.seed_genesis(&boxes).unwrap();
        }
    }
    fn output(path: &Path) -> (u8, Vec<serde_json::Value>) {
        let mut out = Vec::new();
        let code = scan(path, &mut out).unwrap();
        (
            code,
            String::from_utf8(out)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect(),
        )
    }
    fn remove(path: &Path, table: Tbl, key: Option<&[u8]>) {
        let db = Database::open(path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(table).unwrap();
            let key = key.map(Vec::from).unwrap_or_else(|| {
                table
                    .iter()
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .0
                    .value()
                    .to_vec()
            });
            assert!(table.remove(key.as_slice()).unwrap().is_some());
        }
        tx.commit().unwrap();
    }
    #[test]
    fn clean_genesis_and_real_partial_store_are_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        for partial in [false, true] {
            let path = dir.path().join(format!("{partial}.redb"));
            fixture(&path, partial);
            let before = std::fs::read(&path).unwrap();
            let (code, rows) = output(&path);
            assert_eq!(code, 0, "{rows:?}");
            assert_eq!(rows.last().unwrap()["findings"], 0);
            assert_eq!(rows.last().unwrap()["partial"], partial);
            assert_eq!(rows.last().unwrap()["coverage_complete"], !partial);
            if partial {
                assert!(
                    rows.last().unwrap()["permitted_gaps"]["P1"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
                assert!(
                    rows.last().unwrap()["permitted_gaps"]["P2"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
            }
            assert_eq!(rows, output(&path).1);
            assert_eq!(before, std::fs::read(path).unwrap());
        }
    }
    #[test]
    fn removed_required_rows_are_named_in_full_and_partial_stores() {
        let dir = tempfile::tempdir().unwrap();
        for (i, table, key, expected, partial) in [
            (0, ERGO_TREES, None, vec![7, 22], false),
            (1, TREE_BALANCE, None, vec![12, 14, 19], false),
            (2, BOXES, None, vec![6, 17], false),
            (3, META, Some(META_NEXT_BOX_GIDX), vec![28], false),
            (4, META, Some(META_EMISSION_TREE_HASH), vec![18], false),
            (5, HEADERS, None, vec![1, 2], true),
            (6, TXS, None, vec![3, 19, 23], true),
            (7, TX_BY_GIDX, None, vec![4], true),
            (8, BOX_BY_GIDX, None, vec![5, 6, 19], true),
            (9, TOKENS, None, vec![10, 25], true),
        ] {
            let path = dir.path().join(format!("{i}.redb"));
            fixture(&path, partial);
            remove(&path, table, key);
            let (code, rows) = output(&path);
            assert_eq!(code, 1);
            for r in expected {
                assert!(
                    rows.iter().any(|v| v["invariant"] == format!("R{r}")),
                    "missing R{r}: {rows:?}"
                );
            }
        }
    }
    #[test]
    fn invalid_json_is_a_finding_but_null_is_optional() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        fixture(&path, false);
        for (json, expected) in [("null", 0), ("{", 1)] {
            let db = Database::open(&path).unwrap();
            let tx = db.begin_write().unwrap();
            {
                let mut boxes = tx.open_table(BOXES).unwrap();
                let (key, mut row) = {
                    let (k, v) = boxes.iter().unwrap().next().unwrap().unwrap();
                    (k.value().to_vec(), BoxRow::decode(v.value()).unwrap())
                };
                row.registers_json = json.into();
                boxes
                    .insert(key.as_slice(), row.encode().as_slice())
                    .unwrap();
            }
            tx.commit().unwrap();
            drop(db);
            let (code, rows) = output(&path);
            assert_eq!(code, expected);
            assert_eq!(
                rows.last().unwrap()["per_invariant"]["R9"]["findings"],
                expected
            );
        }
    }
    #[test]
    fn full_store_missing_inputs_and_mints_are_findings_not_optional_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full.redb");
        fixture(&path, false);
        let db = Database::open(&path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let row = TxRow {
                height: 1,
                index: 0,
                gidx: 0,
                first_out_gidx: 0,
                timestamp: 0,
                size: 0,
                fee: 0,
                inputs: vec![[77; 32]],
                data_inputs: vec![],
                output_count: 0,
            };
            tx.open_table(TXS)
                .unwrap()
                .insert([88; 32].as_slice(), row.encode().as_slice())
                .unwrap();
            let mut boxes = tx.open_table(BOXES).unwrap();
            let (id, mut row) = {
                let (k, v) = boxes.iter().unwrap().next().unwrap().unwrap();
                (k.value().to_vec(), BoxRow::decode(v.value()).unwrap())
            };
            row.tokens.push(([66; 32], 1));
            boxes
                .insert(id.as_slice(), row.encode().as_slice())
                .unwrap();
        }
        tx.commit().unwrap();
        drop(db);
        let (code, rows) = output(&path);
        assert_eq!(code, 1);
        for r in [8, 11, 21, 27] {
            assert!(rows.iter().any(|v| v["invariant"] == format!("R{r}")));
        }
        let path = dir.path().join("optional.redb");
        fixture(&path, true);
        let db = Database::open(&path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut tokens = tx.open_table(TOKENS).unwrap();
            let (id, mut row) = {
                let (k, v) = tokens.iter().unwrap().next().unwrap().unwrap();
                (k.value().to_vec(), TokenRow::decode(v.value()).unwrap())
            };
            row.name.clear();
            row.description.clear();
            row.decimals = None;
            row.token_type = None;
            tokens
                .insert(id.as_slice(), row.encode().as_slice())
                .unwrap();
        }
        tx.commit().unwrap();
        drop(db);
        assert_eq!(output(&path).0, 0);
    }
    #[test]
    fn malformed_row_stops_without_a_completion_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        fixture(&path, true);
        let db = Database::open(&path).unwrap();
        let tx = db.begin_write().unwrap();
        tx.open_table(TXS)
            .unwrap()
            .insert([255; 32].as_slice(), b"broken".as_slice())
            .unwrap();
        tx.commit().unwrap();
        drop(db);
        let mut out = Vec::new();
        assert!(scan(&path, &mut out).is_err());
        assert!(!String::from_utf8(out)
            .unwrap()
            .lines()
            .any(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["type"] == "summary"));
    }
    #[test]
    fn accidental_transaction_cannot_commit_and_missing_path_is_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        fixture(&path, false);
        let before = std::fs::read(&path).unwrap();
        let db = read_only::open_source(&path).unwrap();
        if let Ok(tx) = db.begin_write() {
            tx.open_table(META)
                .unwrap()
                .insert(b"attempt".as_slice(), b"write".as_slice())
                .unwrap();
            assert!(tx.commit().is_err());
        }
        drop(db);
        assert_eq!(before, std::fs::read(path).unwrap());
        let absent = dir.path().join("absent.redb");
        assert!(scan(&absent, &mut Vec::new()).is_err());
        assert!(!absent.exists());
    }
    #[test]
    fn interrupted_output_cannot_return_clean() {
        struct Fails;
        impl Write for Fails {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        fixture(&path, false);
        assert!(scan(&path, &mut Fails).is_err());
    }
    // A separate OS process owns redb's actual lock until its parent releases stdin.
    #[test]
    fn process_child() {
        let Some(path) = std::env::var_os("INTEGRITY_TEST_PATH") else {
            return;
        };
        if std::env::var_os("INTEGRITY_TEST_LOCK").is_some() {
            let _store = Store::open(Path::new(&path)).unwrap();
            println!("LOCK_READY");
            std::io::stdout().flush().unwrap();
            let _ = std::io::stdin().read(&mut [0]);
        } else {
            std::process::exit(
                scan(Path::new(&path), &mut std::io::stdout().lock()).unwrap_or(2) as i32,
            );
        }
    }
    fn child(path: &Path) -> Command {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args(["--exact", "tests::process_child", "--nocapture"])
            .env("INTEGRITY_TEST_PATH", path);
        c
    }
    #[test]
    fn process_exit_codes_and_live_store_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        fixture(&path, false);
        assert_eq!(child(&path).output().unwrap().status.code(), Some(0));
        let mut owner = child(&path)
            .env("INTEGRITY_TEST_LOCK", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = std::io::BufReader::new(owner.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line.contains("LOCK_READY") {
                break;
            }
        }
        assert_eq!(child(&path).output().unwrap().status.code(), Some(2));
        drop(owner.stdin.take());
        assert!(owner.wait().unwrap().success());
        remove(&path, ERGO_TREES, None);
        assert_eq!(child(&path).output().unwrap().status.code(), Some(1));
        assert_eq!(
            child(&dir.path().join("absent"))
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }
}
