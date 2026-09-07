//! Apply-side indexing of script templates and register values.
//!
//! Two schema-v2 indexes hang off a box's *script* rather than its address:
//!
//! * `TEMPLATES` / `TEMPLATE_BOXES` / `TEMPLATE_UNSPENT` group boxes by the ergo tree's
//!   template hash (the tree with its segregated constant segment blanked, as computed by
//!   `xp_wire::tree_info` and stored on [`TreeRow::template_hash`]) — so "every box on this
//!   contract, whatever its parameters" is one prefix scan.
//! * `REGISTER_IDX` maps `(register, blake2b256(raw value bytes), gidx)` to nothing, making
//!   "which boxes carry this exact R4 value" a prefix scan too.
//!
//! [`Extras`] owns those four tables for the duration of one block (or of genesis seeding),
//! together with two per-block caches: tree hash → template hash, so a `TreeRow` is read at
//! most once per distinct tree, and template hash → (working row, row as it stood before the
//! block), so `TEMPLATES` is read and written at most once per template per block and the
//! undo row records each template exactly once — as `new_templates` if the block created it,
//! as `prev_templates` otherwise, never both.

use redb::{ReadableTable, Table, WriteTransaction};
use std::collections::HashMap;
use xp_types::{Gidx, Hash32};
use xp_wire::tree::blake2b256;
use xp_wire::DecodedBox;

use crate::keys::{k_hash_gidx, k_register};
use crate::rows::{TemplateRow, TreeRow};
use crate::tables::*;
use crate::StoreError;

/// Shorthand for this crate's uniform `&[u8] -> &[u8]` table shape.
type Tb<'txn> = Table<'txn, &'static [u8], &'static [u8]>;

/// The lowest and highest non-mandatory register indexes an Ergo box can carry (R0..R3 are
/// the mandatory value/script/tokens/creation-info registers and are not indexed).
const FIRST_REG: u8 = 4;
const LAST_REG: u8 = 9;

/// What [`Extras`] contributes to a block's `UndoRow`. `apply_block` moves these into the
/// row it writes; `Store::seed_genesis` discards them (genesis precedes every block and is
/// never rolled back).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ExtrasUndo {
    pub new_templates: Vec<Hash32>,
    pub prev_templates: Vec<(Hash32, TemplateRow)>,
    pub register_keys: Vec<(u8, Hash32, Gidx)>,
}

pub(crate) struct Extras<'txn> {
    templates: Tb<'txn>,
    template_boxes: Tb<'txn>,
    template_unspent: Tb<'txn>,
    register_idx: Tb<'txn>,
    /// tree hash → template hash, memoised from `ERGO_TREES` for this block.
    of_tree: HashMap<Hash32, Hash32>,
    /// template hash → (row being built, row as stored before this block began).
    rows: HashMap<Hash32, (TemplateRow, Option<TemplateRow>)>,
    undo: ExtrasUndo,
}

impl<'txn> Extras<'txn> {
    /// Opens the four tables this module owns. They must not be opened by the caller as
    /// well: redb allows a table to be open only once per write transaction.
    pub(crate) fn open(txn: &'txn WriteTransaction) -> Result<Self, StoreError> {
        Ok(Extras {
            templates: txn.open_table(TEMPLATES)?,
            template_boxes: txn.open_table(TEMPLATE_BOXES)?,
            template_unspent: txn.open_table(TEMPLATE_UNSPENT)?,
            register_idx: txn.open_table(REGISTER_IDX)?,
            of_tree: HashMap::new(),
            rows: HashMap::new(),
            undo: ExtrasUndo::default(),
        })
    }

    /// The template hash recorded on `tree`'s [`TreeRow`], memoised per block.
    ///
    /// A missing row is corruption, not a tolerated gap: every path that indexes a box
    /// (`apply_block`'s outputs, `seed_genesis`) writes the tree row first, and a spend only
    /// reaches here once its `BoxRow` was found — which implies we indexed the output.
    fn template_of(&mut self, ergo_trees: &Tb<'_>, tree: &Hash32) -> Result<Hash32, StoreError> {
        if let Some(t) = self.of_tree.get(tree) {
            return Ok(*t);
        }
        let row = TreeRow::decode(
            ergo_trees
                .get(tree.as_slice())?
                .ok_or(StoreError::Corrupt("missing tree row for template"))?
                .value(),
        )?;
        self.of_tree.insert(*tree, row.template_hash);
        Ok(row.template_hash)
    }

    /// The cached working [`TemplateRow`] for `tmpl`, loading it from `TEMPLATES` (and
    /// remembering the loaded value as the undo baseline) on first touch this block.
    fn row(
        &mut self,
        tmpl: Hash32,
        height: u32,
        example_tree: &Hash32,
    ) -> Result<&mut TemplateRow, StoreError> {
        if !self.rows.contains_key(&tmpl) {
            let prev = self
                .templates
                .get(tmpl.as_slice())?
                .map(|v| TemplateRow::decode(v.value()))
                .transpose()?;
            let cur = prev.clone().unwrap_or(TemplateRow {
                box_count: 0,
                unspent_count: 0,
                first_seen: height,
                example_tree: *example_tree,
            });
            self.rows.insert(tmpl, (cur, prev));
        }
        Ok(&mut self.rows.get_mut(&tmpl).expect("just inserted").0)
    }

    /// Indexes one newly created box: its template membership and every register it carries.
    pub(crate) fn on_output(
        &mut self,
        ergo_trees: &Tb<'_>,
        height: u32,
        gidx: Gidx,
        o: &DecodedBox,
    ) -> Result<(), StoreError> {
        let tree = o.tree_hash.0;
        let tmpl = self.template_of(ergo_trees, &tree)?;
        let k = k_hash_gidx(&tmpl, gidx);
        self.template_boxes.insert(k.as_slice(), &[][..])?;
        self.template_unspent.insert(k.as_slice(), &[][..])?;
        let row = self.row(tmpl, height, &tree)?;
        row.box_count += 1;
        row.unspent_count += 1;

        for reg in FIRST_REG..=LAST_REG {
            let Some(hex_str) = register_hex(&o.registers_json, reg) else {
                continue;
            };
            let raw = hex::decode(hex_str).map_err(|_| StoreError::Corrupt("bad register hex"))?;
            let value_hash = blake2b256(&raw);
            self.register_idx
                .insert(k_register(reg, &value_hash, gidx).as_slice(), &[][..])?;
            self.undo.register_keys.push((reg, value_hash, gidx));
        }
        Ok(())
    }

    /// Drops a spent box from its template's unspent set. `TEMPLATE_BOXES` and `box_count`
    /// are deliberately untouched: they index every box a template ever had.
    ///
    /// Underflow policy matches `apply::debit_balance` — on a fully-synced store it means our
    /// own bookkeeping is wrong, so it errors; a partial store can legitimately see it.
    pub(crate) fn on_spend(
        &mut self,
        ergo_trees: &Tb<'_>,
        tree: &Hash32,
        gidx: Gidx,
        height: u32,
        partial: bool,
    ) -> Result<(), StoreError> {
        let tmpl = self.template_of(ergo_trees, tree)?;
        self.template_unspent
            .remove(k_hash_gidx(&tmpl, gidx).as_slice())?;
        let row = self.row(tmpl, height, tree)?;
        row.unspent_count = if partial {
            row.unspent_count.saturating_sub(1)
        } else {
            row.unspent_count
                .checked_sub(1)
                .ok_or(StoreError::Corrupt("template unspent underflow"))?
        };
        Ok(())
    }

    /// Flushes the cached template rows and yields the undo bookkeeping. Consumes `self` so
    /// the four tables are closed before the caller opens anything else.
    pub(crate) fn finish(mut self) -> Result<ExtrasUndo, StoreError> {
        for (tmpl, (row, prev)) in std::mem::take(&mut self.rows) {
            match prev {
                Some(p) => self.undo.prev_templates.push((tmpl, p)),
                None => self.undo.new_templates.push(tmpl),
            }
            self.templates
                .insert(tmpl.as_slice(), row.encode().as_slice())?;
        }
        Ok(self.undo)
    }
}

/// The raw hex of register `reg` in a box's `registers_json`, or `None` if it is absent.
///
/// `registers_json` is never arbitrary JSON: `xp_wire` renders it itself as a flat
/// `{"R4":"<hex>",…}` object whose values are the node's hex strings, which can contain
/// neither a quote nor a backslash. Scanning for `"R<n>":"` is therefore exact, and saves
/// pulling a JSON parser into this crate for six lookups per box.
fn register_hex(registers_json: &str, reg: u8) -> Option<&str> {
    let (_, after) = registers_json.split_once(&format!("\"R{reg}\":\""))?;
    let end = after.find('"')?;
    Some(&after[..end])
}

#[cfg(test)]
mod tests {
    use super::register_hex;

    #[test]
    fn register_hex_reads_each_present_register_and_nothing_else() {
        let json = r#"{"R4":"0e0401020304","R5":"05a0b1","R6":"0402"}"#;
        assert_eq!(register_hex(json, 4), Some("0e0401020304"));
        assert_eq!(register_hex(json, 5), Some("05a0b1"));
        assert_eq!(register_hex(json, 6), Some("0402"));
        assert_eq!(register_hex(json, 7), None);
        assert_eq!(register_hex(json, 9), None);
        assert_eq!(register_hex("{}", 4), None);
    }
}
