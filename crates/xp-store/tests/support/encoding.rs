//! Independent, deliberately simple schema-v2 fixture serializer. No production
//! encode/key/apply helpers compute expected bytes. Row structs are just data.
use xp_store::rows::*;
pub trait Bytes {
    fn bytes(&self) -> Vec<u8>;
}
fn list<T>(xs: &[T], f: impl Fn(&T) -> Vec<u8>) -> Vec<u8> {
    [
        (xs.len() as u32).to_be_bytes().to_vec(),
        xs.iter().flat_map(f).collect(),
    ]
    .concat()
}
fn string(s: &str) -> Vec<u8> {
    [(s.len() as u32).to_be_bytes().as_slice(), s.as_bytes()].concat()
}
fn opt<T>(x: &Option<T>, f: impl Fn(&T) -> Vec<u8>) -> Vec<u8> {
    match x {
        None => vec![0],
        Some(x) => [vec![1], f(x)].concat(),
    }
}
fn hashes(xs: &[[u8; 32]]) -> Vec<u8> {
    list(xs, |x| x.to_vec())
}
fn tokens(xs: &[([u8; 32], u64)]) -> Vec<u8> {
    list(xs, |(id, n)| [id.as_slice(), &n.to_be_bytes()].concat())
}
impl Bytes for HeaderRow {
    fn bytes(&self) -> Vec<u8> {
        [
            self.id.to_vec(),
            self.parent_id.to_vec(),
            self.timestamp.to_be_bytes().to_vec(),
            self.difficulty.to_be_bytes().to_vec(),
            self.miner_pk.to_vec(),
            self.tx_count.to_be_bytes().to_vec(),
            self.first_tx_gidx.to_be_bytes().to_vec(),
            self.size.to_be_bytes().to_vec(),
            self.fees.to_be_bytes().to_vec(),
            self.reward.to_be_bytes().to_vec(),
            vec![self.version],
            string(&self.raw_json),
        ]
        .concat()
    }
}
impl Bytes for TxRow {
    fn bytes(&self) -> Vec<u8> {
        [
            self.height.to_be_bytes().to_vec(),
            self.index.to_be_bytes().to_vec(),
            self.gidx.to_be_bytes().to_vec(),
            self.first_out_gidx.to_be_bytes().to_vec(),
            self.timestamp.to_be_bytes().to_vec(),
            self.size.to_be_bytes().to_vec(),
            self.fee.to_be_bytes().to_vec(),
            hashes(&self.inputs),
            hashes(&self.data_inputs),
            self.output_count.to_be_bytes().to_vec(),
        ]
        .concat()
    }
}
impl Bytes for BoxRow {
    fn bytes(&self) -> Vec<u8> {
        [
            self.gidx.to_be_bytes().to_vec(),
            self.value.to_be_bytes().to_vec(),
            self.tree_hash.to_vec(),
            self.creation_height.to_be_bytes().to_vec(),
            self.tx_id.to_vec(),
            self.index.to_be_bytes().to_vec(),
            self.size.to_be_bytes().to_vec(),
            tokens(&self.tokens),
            string(&self.registers_json),
            opt(&self.spent, |(tx, h)| {
                [tx.as_slice(), &h.to_be_bytes()].concat()
            }),
        ]
        .concat()
    }
}
impl Bytes for BalanceRow {
    fn bytes(&self) -> Vec<u8> {
        [
            self.nano.to_be_bytes().to_vec(),
            tokens(&self.tokens),
            self.box_count.to_be_bytes().to_vec(),
            self.first_seen.to_be_bytes().to_vec(),
            self.last_seen.to_be_bytes().to_vec(),
            self.tx_count.to_be_bytes().to_vec(),
        ]
        .concat()
    }
}
impl Bytes for TemplateRow {
    fn bytes(&self) -> Vec<u8> {
        [
            self.box_count.to_be_bytes().as_slice(),
            &self.unspent_count.to_be_bytes(),
            &self.first_seen.to_be_bytes(),
            &self.example_tree,
        ]
        .concat()
    }
}
impl Bytes for TreeRow {
    fn bytes(&self) -> Vec<u8> {
        [
            (self.tree_bytes.len() as u32).to_be_bytes().to_vec(),
            self.tree_bytes.clone(),
            self.template_hash.to_vec(),
            string(&self.address),
            vec![self.kind],
        ]
        .concat()
    }
}
impl Bytes for TokenRow {
    fn bytes(&self) -> Vec<u8> {
        [
            self.mint_tx.to_vec(),
            self.mint_box.to_vec(),
            self.mint_height.to_be_bytes().to_vec(),
            self.mint_gidx.to_be_bytes().to_vec(),
            string(&self.name),
            string(&self.description),
            opt(&self.decimals, |x| vec![*x]),
            opt(&self.token_type, |x| string(x)),
            self.emission.to_be_bytes().to_vec(),
            self.burned.to_be_bytes().to_vec(),
            self.holder_count.to_be_bytes().to_vec(),
            self.box_count.to_be_bytes().to_vec(),
        ]
        .concat()
    }
}
impl Bytes for UndoRow {
    fn bytes(&self) -> Vec<u8> {
        [
            hashes(&self.created_boxes),
            hashes(&self.spent_boxes),
            hashes(&self.tx_ids),
            tokens(&self.tree_txs),
            list(&self.prev_balances, |(id, r)| {
                [id.to_vec(), opt(r, Bytes::bytes)].concat()
            }),
            self.prev_next_box_gidx.to_be_bytes().to_vec(),
            self.prev_next_tx_gidx.to_be_bytes().to_vec(),
            hashes(&self.new_trees),
            hashes(&self.new_templates),
            list(&self.prev_templates, |(id, r)| {
                [id.to_vec(), r.bytes()].concat()
            }),
            hashes(&self.new_tokens),
            list(&self.prev_tokens, |(id, r)| {
                [id.to_vec(), r.bytes()].concat()
            }),
            list(&self.prev_holder_amts, |(id, t, n)| {
                [
                    id.to_vec(),
                    t.to_vec(),
                    opt(n, |n| n.to_be_bytes().to_vec()),
                ]
                .concat()
            }),
            list(&self.register_keys, |(r, h, g)| {
                [vec![*r], h.to_vec(), g.to_be_bytes().to_vec()].concat()
            }),
        ]
        .concat()
    }
}
