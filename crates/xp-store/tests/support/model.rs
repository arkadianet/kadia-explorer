//! Independent transition oracle: fixture UTXOs and transaction membership are the
//! source of truth. Secondary indexes/counters are projections of those maps.
//! Rollback replays the retained fixture chain from its initial state; it never
//! reads production UNDO or calls Store, apply, rollback, Extras, or Tokens.
use super::{encoding::Bytes, logical::Snapshot};
use redb::TableHandle;
use std::collections::{BTreeMap, BTreeSet};
use xp_store::{rows::*, tables::*, ROLLBACK_WINDOW};
use xp_types::{BoxId, Hash32, TreeHash, TxId};
use xp_wire::{DecodedBlock, DecodedBox};

pub fn id(n: u64) -> Hash32 {
    let mut x = [0; 32];
    x[..8].copy_from_slice(&n.to_be_bytes());
    x
}
pub fn output(n: u64, tree: u64, height: u32, value: u64) -> DecodedBox {
    DecodedBox {
        id: BoxId(id(n)),
        value,
        tree_bytes: vec![],
        tree_hash: TreeHash(id(tree)),
        creation_height: height,
        tx_id: TxId(id(n + 1)),
        index: 0,
        tokens: vec![],
        registers_json: "{\"R9\":\"0402\"}".into(),
        id_verified: true,
        size: 80,
    }
}
pub fn register_hash() -> Hash32 {
    // Cryptographic primitive only, not production register extraction/indexing.
    use blake2::{digest::consts::U32, Blake2b, Digest};
    Blake2b::<U32>::digest([4, 2]).into()
}
fn pair(hash: &Hash32, n: u64) -> Vec<u8> {
    [hash.as_slice(), &n.to_be_bytes()].concat()
}
fn put(s: &mut Snapshot, t: Tbl, k: impl AsRef<[u8]>, v: impl AsRef<[u8]>) {
    s.get_mut(t.name())
        .unwrap()
        .insert(k.as_ref().to_vec(), v.as_ref().to_vec());
}

#[derive(Clone)]
pub struct Model {
    pub partial: bool,
    pub base: u32,
    pub height: u32,
    pub tip: Hash32,
    pub next_box: u64,
    pub next_tx: u64,
    pub boxes: BTreeMap<Hash32, BoxRow>,
    balances: BTreeMap<Hash32, BalanceRow>,
    trees: BTreeSet<Hash32>,
    template: Option<TemplateRow>,
    tokens: BTreeMap<Hash32, TokenRow>,
    txs: BTreeMap<Hash32, TxRow>,
    tree_txs: BTreeSet<(Hash32, u64)>,
    headers: BTreeMap<u32, HeaderRow>,
    pub undo: BTreeMap<u32, Vec<u8>>,
    chain: Vec<DecodedBlock>,
    tx_meta: bool,
}
impl Model {
    pub fn new(partial: bool, base: u32) -> Self {
        let mut m = Self {
            partial,
            base,
            height: base,
            tip: id(9),
            next_box: 0,
            next_tx: 0,
            boxes: BTreeMap::new(),
            balances: BTreeMap::new(),
            trees: BTreeSet::new(),
            template: None,
            tokens: BTreeMap::new(),
            txs: BTreeMap::new(),
            tree_txs: BTreeSet::new(),
            headers: BTreeMap::new(),
            undo: BTreeMap::new(),
            chain: vec![],
            tx_meta: partial,
        };
        if partial {
            m.headers.insert(
                base,
                HeaderRow {
                    id: id(9),
                    parent_id: [0; 32],
                    timestamp: 0,
                    difficulty: 0,
                    miner_pk: [0; 33],
                    tx_count: 0,
                    first_tx_gidx: 0,
                    size: 0,
                    fees: 0,
                    reward: 0,
                    version: 0,
                    raw_json: String::new(),
                },
            );
        } else {
            m.add_box(&output(100, 1, 0, 1_000_000));
            m.refresh(&BTreeSet::from([id(1)]), 0);
        }
        m
    }
    fn add_box(&mut self, o: &DecodedBox) {
        self.boxes.insert(
            o.id.0,
            BoxRow {
                gidx: self.next_box,
                value: o.value,
                tree_hash: o.tree_hash.0,
                creation_height: o.creation_height,
                tx_id: o.tx_id.0,
                index: o.index,
                size: o.size,
                tokens: o.tokens.clone(),
                registers_json: o.registers_json.clone(),
                spent: None,
            },
        );
        self.next_box += 1;
        self.trees.insert(o.tree_hash.0);
        self.template.get_or_insert(TemplateRow {
            box_count: 0,
            unspent_count: 0,
            first_seen: self.height,
            example_tree: o.tree_hash.0,
        });
    }
    fn holders(&self) -> BTreeMap<(Hash32, Hash32), u64> {
        let mut h = BTreeMap::new();
        for b in self.boxes.values().filter(|b| b.spent.is_none()) {
            for (t, n) in &b.tokens {
                *h.entry((*t, b.tree_hash)).or_default() += n;
            }
        }
        h.retain(|_, n| *n > 0);
        h
    }
    fn refresh(&mut self, touched: &BTreeSet<Hash32>, height: u32) {
        for tree in touched {
            let live: Vec<_> = self
                .boxes
                .values()
                .filter(|b| b.tree_hash == *tree && b.spent.is_none())
                .collect();
            let mut amounts = BTreeMap::new();
            for b in &live {
                for (t, n) in &b.tokens {
                    *amounts.entry(*t).or_default() += n;
                }
            }
            // Generated fixtures have at most one live token species; vector order
            // therefore has no dependency on the production swap-remove algorithm.
            assert!(amounts.len() <= 1);
            let first = self.balances.get(tree).map_or(height, |b| b.first_seen);
            self.balances.insert(
                *tree,
                BalanceRow {
                    nano: live.iter().map(|b| b.value).sum(),
                    tokens: amounts.into_iter().collect(),
                    box_count: live.len() as u64,
                    first_seen: first,
                    last_seen: height,
                    tx_count: self.tree_txs.iter().filter(|(t, _)| t == tree).count() as u64,
                },
            );
        }
        if let Some(t) = &mut self.template {
            t.box_count = self.boxes.len() as u64;
            t.unspent_count = self.boxes.values().filter(|b| b.spent.is_none()).count() as u64;
        }
        let holders = self.holders();
        for (id, t) in &mut self.tokens {
            t.box_count = self
                .boxes
                .values()
                .filter(|b| b.tokens.iter().any(|(i, _)| i == id))
                .count() as u64;
            t.holder_count = holders.keys().filter(|(i, _)| i == id).count() as u64;
        }
    }
    pub fn advance(&mut self, b: &DecodedBlock) {
        let old = self.clone_without_chain();
        self.height = b.header.height;
        self.tip = b.header.id.0;
        self.tx_meta = true;
        let mut touched = BTreeSet::new();
        let mut touched_holders = BTreeSet::new();
        let mut touched_tokens = BTreeSet::new();
        let mut u = UndoRow {
            created_boxes: vec![],
            spent_boxes: vec![],
            tx_ids: vec![],
            tree_txs: vec![],
            prev_balances: vec![],
            prev_next_box_gidx: self.next_box,
            prev_next_tx_gidx: self.next_tx,
            new_trees: vec![],
            new_templates: vec![],
            prev_templates: vec![],
            new_tokens: vec![],
            prev_tokens: vec![],
            prev_holder_amts: vec![],
            register_keys: vec![],
        };
        for (index, tx) in b.txs.iter().enumerate() {
            let mut tx_trees = BTreeSet::new();
            let mut consumed = BTreeMap::<Hash32, u64>::new();
            let start = self.next_box;
            for inp in &tx.inputs {
                if let Some(row) = self.boxes.get_mut(&inp.0) {
                    assert!(row.spent.is_none());
                    row.spent = Some((tx.id.0, self.height));
                    tx_trees.insert(row.tree_hash);
                    u.spent_boxes.push(inp.0);
                    for (t, n) in &row.tokens {
                        *consumed.entry(*t).or_default() += n;
                        touched_holders.insert((*t, row.tree_hash));
                    }
                } else {
                    assert!(self.partial);
                }
            }
            let mut produced = BTreeMap::<Hash32, u64>::new();
            for o in &tx.outputs {
                if !self.trees.contains(&o.tree_hash.0) {
                    u.new_trees.push(o.tree_hash.0);
                }
                u.created_boxes.push(o.id.0);
                u.register_keys.push((9, register_hash(), self.next_box));
                tx_trees.insert(o.tree_hash.0);
                for (t, n) in &o.tokens {
                    *produced.entry(*t).or_default() += n;
                    touched_holders.insert((*t, o.tree_hash.0));
                    touched_tokens.insert(*t);
                }
                self.add_box(o);
            }
            if let Some(first) = tx.inputs.first() {
                if let Some(emission) = produced.get(&first.0) {
                    if !self.tokens.contains_key(&first.0) {
                        let (i, o) = tx
                            .outputs
                            .iter()
                            .enumerate()
                            .find(|(_, o)| o.tokens.iter().any(|(t, _)| *t == first.0))
                            .unwrap();
                        self.tokens.insert(
                            first.0,
                            TokenRow {
                                mint_tx: tx.id.0,
                                mint_box: o.id.0,
                                mint_height: self.height,
                                mint_gidx: start + i as u64,
                                name: String::new(),
                                description: String::new(),
                                decimals: None,
                                token_type: None,
                                emission: *emission,
                                burned: 0,
                                holder_count: 0,
                                box_count: 0,
                            },
                        );
                    }
                }
            }
            for (t, n) in consumed {
                let burn = n.saturating_sub(*produced.get(&t).unwrap_or(&0));
                if burn > 0 {
                    touched_tokens.insert(t);
                    if let Some(row) = self.tokens.get_mut(&t) {
                        row.burned += burn;
                    }
                }
            }
            for tree in tx_trees {
                touched.insert(tree);
                self.tree_txs.insert((tree, self.next_tx));
                u.tree_txs.push((tree, self.next_tx));
            }
            self.txs.insert(
                tx.id.0,
                TxRow {
                    height: self.height,
                    index: index as u16,
                    gidx: self.next_tx,
                    first_out_gidx: start,
                    timestamp: b.header.timestamp,
                    size: tx.size,
                    fee: 0,
                    inputs: tx.inputs.iter().map(|i| i.0).collect(),
                    data_inputs: tx.data_inputs.iter().map(|i| i.0).collect(),
                    output_count: tx.outputs.len() as u16,
                },
            );
            self.next_tx += 1;
            u.tx_ids.push(tx.id.0);
        }
        self.refresh(&touched, self.height);
        for t in &touched {
            u.prev_balances.push((*t, old.balances.get(t).cloned()));
        }
        if !touched.is_empty() {
            match &old.template {
                Some(t) => u.prev_templates.push(([0; 32], t.clone())),
                None => u.new_templates.push([0; 32]),
            }
        }
        let old_holders = old.holders();
        let new_holders = self.holders();
        for (t, tree) in touched_holders {
            u.prev_holder_amts
                .push((t, tree, old_holders.get(&(t, tree)).copied()));
            if old_holders.contains_key(&(t, tree)) != new_holders.contains_key(&(t, tree)) {
                touched_tokens.insert(t);
            }
        }
        for t in touched_tokens {
            match old.tokens.get(&t) {
                Some(r) => u.prev_tokens.push((t, r.clone())),
                None if self.tokens.contains_key(&t) => u.new_tokens.push(t),
                None => {}
            }
        }
        let reward = b.txs.first().map_or(0, |tx| {
            let input_tree = tx
                .inputs
                .first()
                .and_then(|i| self.boxes.get(&i.0))
                .map(|r| r.tree_hash);
            match input_tree {
                Some(tree) => tx
                    .outputs
                    .iter()
                    .filter(|o| o.tree_hash.0 != tree)
                    .map(|o| o.value)
                    .sum(),
                None => {
                    tx.outputs.iter().map(|o| o.value).sum::<u64>()
                        - tx.outputs.iter().map(|o| o.value).max().unwrap_or(0)
                }
            }
        });
        self.headers.insert(
            self.height,
            HeaderRow {
                id: self.tip,
                parent_id: b.header.parent_id.0,
                timestamp: b.header.timestamp,
                difficulty: b.header.difficulty,
                miner_pk: b.header.miner_pk,
                tx_count: b.txs.len() as u32,
                first_tx_gidx: old.next_tx,
                size: b.size,
                fees: 0,
                reward,
                version: b.header.version,
                raw_json: b.header.raw_json.clone(),
            },
        );
        self.undo.insert(self.height, u.bytes());
        // Current contract retains W+1 rows, though rollback distance is capped at W.
        self.undo
            .retain(|h, _| *h >= self.height.saturating_sub(ROLLBACK_WINDOW));
        self.chain.push(b.clone());
    }
    fn clone_without_chain(&self) -> Self {
        // Only pre-state maps used for undo expectations, never database snapshots.
        let mut m = Self::new(self.partial, self.base);
        m.balances = self.balances.clone();
        m.template = self.template.clone();
        m.tokens = self.tokens.clone();
        m.boxes = self.boxes.clone();
        m.next_tx = self.next_tx;
        m
    }
    pub fn rewind(&mut self, target: u32) {
        let chain: Vec<_> = self
            .chain
            .iter()
            .filter(|b| b.header.height <= target)
            .cloned()
            .collect();
        let retained: BTreeSet<_> = self.undo.keys().copied().filter(|h| *h <= target).collect();
        let mut fresh = Self::new(self.partial, self.base);
        for b in chain {
            fresh.advance(&b);
        }
        fresh.undo.retain(|h, _| retained.contains(h));
        fresh.tx_meta = true; // rollback writes the restored counter, including at genesis.
        *self = fresh;
    }
    pub fn expected(&self) -> Snapshot {
        let mut s: Snapshot = ALL
            .into_iter()
            .map(|t| (t.name().to_owned(), BTreeMap::new()))
            .collect();
        put(&mut s, META, META_SCHEMA, 2u32.to_be_bytes());
        put(
            &mut s,
            META,
            META_NEXT_BOX_GIDX,
            self.next_box.to_be_bytes(),
        );
        if self.tx_meta {
            put(&mut s, META, META_NEXT_TX_GIDX, self.next_tx.to_be_bytes());
        }
        if self.partial {
            put(&mut s, META, META_PARTIAL_FROM, self.base.to_be_bytes());
        } else {
            put(&mut s, META, META_GENESIS_SEEDED, [1]);
            put(&mut s, META, META_EMISSION_TREE_HASH, id(1));
        }
        if self.height > 0 {
            put(&mut s, META, META_INDEXED_HEIGHT, self.height.to_be_bytes());
        }
        for (h, r) in &self.headers {
            put(&mut s, HEADERS, h.to_be_bytes(), r.bytes());
            put(&mut s, HEADER_BY_ID, r.id, h.to_be_bytes());
        }
        for (id, r) in &self.txs {
            put(&mut s, TXS, id, r.bytes());
            put(&mut s, TX_BY_GIDX, r.gidx.to_be_bytes(), id);
        }
        for tree in &self.trees {
            put(
                &mut s,
                ERGO_TREES,
                tree,
                TreeRow {
                    tree_bytes: vec![],
                    template_hash: [0; 32],
                    address: String::new(),
                    kind: 2,
                }
                .bytes(),
            );
        }
        for (id, b) in &self.boxes {
            put(&mut s, BOXES, id, b.bytes());
            put(&mut s, BOX_BY_GIDX, b.gidx.to_be_bytes(), id);
            put(&mut s, TREE_BOXES, pair(&b.tree_hash, b.gidx), []);
            put(&mut s, TEMPLATE_BOXES, pair(&[0; 32], b.gidx), []);
            put(
                &mut s,
                REGISTER_IDX,
                [
                    vec![9],
                    register_hash().to_vec(),
                    b.gidx.to_be_bytes().to_vec(),
                ]
                .concat(),
                [],
            );
            if b.spent.is_none() {
                put(&mut s, TREE_UNSPENT, pair(&b.tree_hash, b.gidx), []);
                put(&mut s, TEMPLATE_UNSPENT, pair(&[0; 32], b.gidx), []);
                put(
                    &mut s,
                    RENT_MATURES,
                    [
                        (b.creation_height + 1_051_200).to_be_bytes().as_slice(),
                        &b.gidx.to_be_bytes(),
                    ]
                    .concat(),
                    id,
                );
            }
            for (t, _) in &b.tokens {
                put(&mut s, TOKEN_BOXES, pair(t, b.gidx), []);
                if b.spent.is_none() {
                    put(&mut s, TOKEN_UNSPENT, pair(t, b.gidx), []);
                }
            }
        }
        for (t, g) in &self.tree_txs {
            put(&mut s, TREE_TXS, pair(t, *g), []);
        }
        for (t, b) in &self.balances {
            put(&mut s, TREE_BALANCE, t, b.bytes());
            if b.nano > 0 {
                put(
                    &mut s,
                    RICH,
                    [b.nano.to_be_bytes().as_slice(), t].concat(),
                    [],
                );
            }
        }
        if let Some(t) = &self.template {
            put(&mut s, TEMPLATES, [0; 32], t.bytes());
        }
        for (id, t) in &self.tokens {
            put(&mut s, TOKENS, id, t.bytes());
            put(&mut s, TOKENS_BY_GIDX, t.mint_gidx.to_be_bytes(), id);
            put(
                &mut s,
                TOKENS_BY_HOLDERS,
                [t.holder_count.to_be_bytes().as_slice(), id].concat(),
                [],
            );
        }
        for ((id, tree), n) in self.holders() {
            put(
                &mut s,
                TOKEN_HOLDER_AMT,
                [id, tree].concat(),
                n.to_be_bytes(),
            );
            put(
                &mut s,
                TOKEN_HOLDERS,
                [id.to_vec(), n.to_be_bytes().to_vec(), tree.to_vec()].concat(),
                [],
            );
        }
        for (h, u) in &self.undo {
            put(&mut s, UNDO, h.to_be_bytes(), u);
        }
        s
    }
}
