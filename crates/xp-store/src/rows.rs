use crate::StoreError;
use xp_types::{Gidx, Hash32};

/// Append-only byte-vector writer. Fixed-width ints are big-endian; `bytes`/`str` are
/// prefixed by a big-endian `u32` length; `opt_flag` writes the 1-byte `Option` discriminant.
struct W(Vec<u8>);

impl W {
    fn new() -> Self {
        W(Vec::new())
    }

    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    fn u128(&mut self, v: u128) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    fn raw(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }

    fn hash(&mut self, v: &Hash32) {
        self.raw(v);
    }

    fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.raw(v);
    }

    fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }

    fn opt_flag(&mut self, is_some: bool) {
        self.u8(u8::from(is_some));
    }

    fn hash_vec(&mut self, v: &[Hash32]) {
        self.u32(v.len() as u32);
        for h in v {
            self.hash(h);
        }
    }

    fn token_vec(&mut self, v: &[(Hash32, u64)]) {
        self.u32(v.len() as u32);
        for (h, amt) in v {
            self.hash(h);
            self.u64(*amt);
        }
    }

    fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

/// Cursor reader matching `W`'s encoding. Every read is bounds-checked; truncated input
/// fails with `StoreError::Corrupt` instead of panicking.
struct R<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> R<'a> {
    fn new(buf: &'a [u8]) -> Self {
        R { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], StoreError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.buf.len())
            .ok_or(StoreError::Corrupt("unexpected end of row"))?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, StoreError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, StoreError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, StoreError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, StoreError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn u128(&mut self) -> Result<u128, StoreError> {
        Ok(u128::from_be_bytes(self.take(16)?.try_into().unwrap()))
    }

    fn hash(&mut self) -> Result<Hash32, StoreError> {
        Ok(self.take(32)?.try_into().unwrap())
    }

    fn bytes(&mut self) -> Result<Vec<u8>, StoreError> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }

    fn str(&mut self) -> Result<String, StoreError> {
        String::from_utf8(self.bytes()?).map_err(|_| StoreError::Corrupt("invalid utf8 in row"))
    }

    fn opt_flag(&mut self) -> Result<bool, StoreError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StoreError::Corrupt("bad Option discriminant")),
        }
    }

    fn hash_vec(&mut self) -> Result<Vec<Hash32>, StoreError> {
        let n = self.u32()? as usize;
        let mut v = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            v.push(self.hash()?);
        }
        Ok(v)
    }

    fn token_vec(&mut self) -> Result<Vec<(Hash32, u64)>, StoreError> {
        let n = self.u32()? as usize;
        let mut v = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            let h = self.hash()?;
            let amt = self.u64()?;
            v.push((h, amt));
        }
        Ok(v)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderRow {
    pub id: Hash32,
    pub parent_id: Hash32,
    pub timestamp: u64,
    pub difficulty: u128,
    pub miner_pk: [u8; 33],
    pub tx_count: u32,
    /// `TxRow.gidx` of this block's first tx, so `Reader::txs_in_block` can locate the
    /// block's tx range without an O(chain) scan (see Task 6 interface amendment).
    pub first_tx_gidx: Gidx,
    pub size: u32,
    pub fees: u64,
    pub reward: u64,
    pub version: u8,
    pub raw_json: String,
}

impl HeaderRow {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::new();
        w.hash(&self.id);
        w.hash(&self.parent_id);
        w.u64(self.timestamp);
        w.u128(self.difficulty);
        w.raw(&self.miner_pk);
        w.u32(self.tx_count);
        w.u64(self.first_tx_gidx);
        w.u32(self.size);
        w.u64(self.fees);
        w.u64(self.reward);
        w.u8(self.version);
        w.str(&self.raw_json);
        w.into_vec()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StoreError> {
        let mut r = R::new(buf);
        Ok(HeaderRow {
            id: r.hash()?,
            parent_id: r.hash()?,
            timestamp: r.u64()?,
            difficulty: r.u128()?,
            miner_pk: r.take(33)?.try_into().unwrap(),
            tx_count: r.u32()?,
            first_tx_gidx: r.u64()?,
            size: r.u32()?,
            fees: r.u64()?,
            reward: r.u64()?,
            version: r.u8()?,
            raw_json: r.str()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxRow {
    pub height: u32,
    pub index: u16,
    pub gidx: Gidx,
    pub first_out_gidx: Gidx,
    pub timestamp: u64,
    pub size: u32,
    pub fee: u64,
    pub inputs: Vec<Hash32>,
    pub data_inputs: Vec<Hash32>,
    pub output_count: u16,
}

impl TxRow {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::new();
        w.u32(self.height);
        w.u16(self.index);
        w.u64(self.gidx);
        w.u64(self.first_out_gidx);
        w.u64(self.timestamp);
        w.u32(self.size);
        w.u64(self.fee);
        w.hash_vec(&self.inputs);
        w.hash_vec(&self.data_inputs);
        w.u16(self.output_count);
        w.into_vec()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StoreError> {
        let mut r = R::new(buf);
        Ok(TxRow {
            height: r.u32()?,
            index: r.u16()?,
            gidx: r.u64()?,
            first_out_gidx: r.u64()?,
            timestamp: r.u64()?,
            size: r.u32()?,
            fee: r.u64()?,
            inputs: r.hash_vec()?,
            data_inputs: r.hash_vec()?,
            output_count: r.u16()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxRow {
    pub gidx: Gidx,
    pub value: u64,
    pub tree_hash: Hash32,
    pub creation_height: u32,
    pub tx_id: Hash32,
    pub index: u16,
    pub size: u32,
    pub tokens: Vec<(Hash32, u64)>,
    pub registers_json: String,
    pub spent: Option<(Hash32, u32)>,
}

impl BoxRow {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::new();
        w.u64(self.gidx);
        w.u64(self.value);
        w.hash(&self.tree_hash);
        w.u32(self.creation_height);
        w.hash(&self.tx_id);
        w.u16(self.index);
        w.u32(self.size);
        w.token_vec(&self.tokens);
        w.str(&self.registers_json);
        w.opt_flag(self.spent.is_some());
        if let Some((tx, height)) = &self.spent {
            w.hash(tx);
            w.u32(*height);
        }
        w.into_vec()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StoreError> {
        let mut r = R::new(buf);
        let gidx = r.u64()?;
        let value = r.u64()?;
        let tree_hash = r.hash()?;
        let creation_height = r.u32()?;
        let tx_id = r.hash()?;
        let index = r.u16()?;
        let size = r.u32()?;
        let tokens = r.token_vec()?;
        let registers_json = r.str()?;
        let spent = if r.opt_flag()? {
            Some((r.hash()?, r.u32()?))
        } else {
            None
        };
        Ok(BoxRow {
            gidx,
            value,
            tree_hash,
            creation_height,
            tx_id,
            index,
            size,
            tokens,
            registers_json,
            spent,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub tree_bytes: Vec<u8>,
    pub template_hash: Hash32,
    pub address: String,
    pub kind: u8,
}

impl TreeRow {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::new();
        w.bytes(&self.tree_bytes);
        w.hash(&self.template_hash);
        w.str(&self.address);
        w.u8(self.kind);
        w.into_vec()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StoreError> {
        let mut r = R::new(buf);
        Ok(TreeRow {
            tree_bytes: r.bytes()?,
            template_hash: r.hash()?,
            address: r.str()?,
            kind: r.u8()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceRow {
    pub nano: u64,
    pub tokens: Vec<(Hash32, u64)>,
    pub box_count: u64,
    pub first_seen: u32,
    pub last_seen: u32,
}

impl BalanceRow {
    fn encode_into(&self, w: &mut W) {
        w.u64(self.nano);
        w.token_vec(&self.tokens);
        w.u64(self.box_count);
        w.u32(self.first_seen);
        w.u32(self.last_seen);
    }

    fn decode_from(r: &mut R) -> Result<Self, StoreError> {
        Ok(BalanceRow {
            nano: r.u64()?,
            tokens: r.token_vec()?,
            box_count: r.u64()?,
            first_seen: r.u32()?,
            last_seen: r.u32()?,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::new();
        self.encode_into(&mut w);
        w.into_vec()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StoreError> {
        let mut r = R::new(buf);
        Self::decode_from(&mut r)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoRow {
    pub created_boxes: Vec<Hash32>,
    pub spent_boxes: Vec<Hash32>,
    pub tx_ids: Vec<Hash32>,
    /// `(tree_hash, tx_gidx)` pairs inserted into `TREE_TXS` by this block, so rollback can
    /// remove exactly the keys this block added.
    pub tree_txs: Vec<(Hash32, Gidx)>,
    pub prev_balances: Vec<(Hash32, Option<BalanceRow>)>,
    pub prev_next_box_gidx: Gidx,
    pub prev_next_tx_gidx: Gidx,
    pub new_trees: Vec<Hash32>,
}

impl UndoRow {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::new();
        w.hash_vec(&self.created_boxes);
        w.hash_vec(&self.spent_boxes);
        w.hash_vec(&self.tx_ids);
        w.token_vec(&self.tree_txs);
        w.u32(self.prev_balances.len() as u32);
        for (tree, bal) in &self.prev_balances {
            w.hash(tree);
            w.opt_flag(bal.is_some());
            if let Some(bal) = bal {
                bal.encode_into(&mut w);
            }
        }
        w.u64(self.prev_next_box_gidx);
        w.u64(self.prev_next_tx_gidx);
        w.hash_vec(&self.new_trees);
        w.into_vec()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StoreError> {
        let mut r = R::new(buf);
        let created_boxes = r.hash_vec()?;
        let spent_boxes = r.hash_vec()?;
        let tx_ids = r.hash_vec()?;
        let tree_txs = r.token_vec()?;
        let n = r.u32()? as usize;
        let mut prev_balances = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            let tree = r.hash()?;
            let bal = if r.opt_flag()? {
                Some(BalanceRow::decode_from(&mut r)?)
            } else {
                None
            };
            prev_balances.push((tree, bal));
        }
        let prev_next_box_gidx = r.u64()?;
        let prev_next_tx_gidx = r.u64()?;
        let new_trees = r.hash_vec()?;
        Ok(UndoRow {
            created_boxes,
            spent_boxes,
            tx_ids,
            tree_txs,
            prev_balances,
            prev_next_box_gidx,
            prev_next_tx_gidx,
            new_trees,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_hash() -> impl Strategy<Value = [u8; 32]> {
        any::<[u8; 32]>()
    }

    proptest! {
        #[test]
        fn box_row_roundtrip(gidx in any::<u64>(), value in any::<u64>(), th in arb_hash(), ch in any::<u32>(), tx in arb_hash(),
                             idx in any::<u16>(), size in any::<u32>(), toks in proptest::collection::vec((arb_hash(), any::<u64>()), 0..4),
                             regs in ".*", spent in proptest::option::of((arb_hash(), any::<u32>()))) {
            let r = BoxRow { gidx, value, tree_hash: th, creation_height: ch, tx_id: tx, index: idx, size, tokens: toks, registers_json: regs, spent };
            let d = BoxRow::decode(&r.encode()).unwrap();
            prop_assert_eq!(d, r);
        }
        #[test]
        fn balance_row_roundtrip(nano in any::<u64>(), toks in proptest::collection::vec((arb_hash(), any::<u64>()), 0..4), bc in any::<u64>(), f in any::<u32>(), l in any::<u32>()) {
            let r = BalanceRow { nano, tokens: toks, box_count: bc, first_seen: f, last_seen: l };
            prop_assert_eq!(BalanceRow::decode(&r.encode()).unwrap(), r);
        }
        #[test]
        fn header_row_roundtrip(id in arb_hash(), parent_id in arb_hash(), timestamp in any::<u64>(), difficulty in any::<u128>(),
                                miner_pk in any::<[u8; 33]>(), tx_count in any::<u32>(), first_tx_gidx in any::<u64>(), size in any::<u32>(), fees in any::<u64>(),
                                reward in any::<u64>(), version in any::<u8>(), raw_json in ".*") {
            let r = HeaderRow { id, parent_id, timestamp, difficulty, miner_pk, tx_count, first_tx_gidx, size, fees, reward, version, raw_json };
            prop_assert_eq!(HeaderRow::decode(&r.encode()).unwrap(), r);
        }
        #[test]
        fn tx_row_roundtrip(height in any::<u32>(), index in any::<u16>(), gidx in any::<u64>(), first_out_gidx in any::<u64>(),
                            timestamp in any::<u64>(), size in any::<u32>(), fee in any::<u64>(),
                            inputs in proptest::collection::vec(arb_hash(), 0..4), data_inputs in proptest::collection::vec(arb_hash(), 0..4),
                            output_count in any::<u16>()) {
            let r = TxRow { height, index, gidx, first_out_gidx, timestamp, size, fee, inputs, data_inputs, output_count };
            prop_assert_eq!(TxRow::decode(&r.encode()).unwrap(), r);
        }
        #[test]
        fn tree_row_roundtrip(tree_bytes in proptest::collection::vec(any::<u8>(), 0..16), template_hash in arb_hash(), address in ".*", kind in any::<u8>()) {
            let r = TreeRow { tree_bytes, template_hash, address, kind };
            prop_assert_eq!(TreeRow::decode(&r.encode()).unwrap(), r);
        }
    }

    #[test]
    fn undo_row_roundtrip() {
        let r = UndoRow {
            created_boxes: vec![[1; 32]],
            spent_boxes: vec![[2; 32], [3; 32]],
            tx_ids: vec![[4; 32]],
            tree_txs: vec![([9; 32], 42), ([10; 32], 43)],
            prev_balances: vec![
                ([5; 32], None),
                (
                    [6; 32],
                    Some(BalanceRow {
                        nano: 7,
                        tokens: vec![],
                        box_count: 1,
                        first_seen: 1,
                        last_seen: 2,
                    }),
                ),
            ],
            prev_next_box_gidx: 10,
            prev_next_tx_gidx: 11,
            new_trees: vec![[8; 32]],
        };
        assert_eq!(UndoRow::decode(&r.encode()).unwrap(), r);
    }

    #[test]
    fn header_row_decode_truncated_is_corrupt() {
        let r = HeaderRow {
            id: [1; 32],
            parent_id: [2; 32],
            timestamp: 3,
            difficulty: 4,
            miner_pk: [5; 33],
            tx_count: 6,
            first_tx_gidx: 7,
            size: 8,
            fees: 9,
            reward: 10,
            version: 1,
            raw_json: "{}".into(),
        };
        let full = r.encode();
        for cut in [0, 1, 32, 64, 90, full.len() - 1] {
            assert!(matches!(
                HeaderRow::decode(&full[..cut]),
                Err(StoreError::Corrupt(_))
            ));
        }
    }

    #[test]
    fn tx_row_decode_truncated_is_corrupt() {
        let r = TxRow {
            height: 1,
            index: 2,
            gidx: 3,
            first_out_gidx: 4,
            timestamp: 5,
            size: 6,
            fee: 7,
            inputs: vec![[9; 32]],
            data_inputs: vec![],
            output_count: 1,
        };
        let full = r.encode();
        assert!(matches!(
            TxRow::decode(&full[..full.len() - 1]),
            Err(StoreError::Corrupt(_))
        ));
        assert!(matches!(TxRow::decode(&[]), Err(StoreError::Corrupt(_))));
    }

    #[test]
    fn box_row_decode_truncated_is_corrupt() {
        let r = BoxRow {
            gidx: 1,
            value: 2,
            tree_hash: [3; 32],
            creation_height: 4,
            tx_id: [5; 32],
            index: 6,
            size: 7,
            tokens: vec![([8; 32], 9)],
            registers_json: "{}".into(),
            spent: Some(([10; 32], 11)),
        };
        let full = r.encode();
        assert!(matches!(
            BoxRow::decode(&full[..full.len() - 1]),
            Err(StoreError::Corrupt(_))
        ));
        assert!(matches!(BoxRow::decode(&[]), Err(StoreError::Corrupt(_))));
    }

    #[test]
    fn tree_row_decode_truncated_is_corrupt() {
        let r = TreeRow {
            tree_bytes: vec![1, 2, 3],
            template_hash: [4; 32],
            address: "9f...".into(),
            kind: 1,
        };
        let full = r.encode();
        assert!(matches!(
            TreeRow::decode(&full[..full.len() - 1]),
            Err(StoreError::Corrupt(_))
        ));
        assert!(matches!(TreeRow::decode(&[]), Err(StoreError::Corrupt(_))));
    }

    #[test]
    fn balance_row_decode_truncated_is_corrupt() {
        let r = BalanceRow {
            nano: 1,
            tokens: vec![([2; 32], 3)],
            box_count: 4,
            first_seen: 5,
            last_seen: 6,
        };
        let full = r.encode();
        assert!(matches!(
            BalanceRow::decode(&full[..full.len() - 1]),
            Err(StoreError::Corrupt(_))
        ));
        assert!(matches!(
            BalanceRow::decode(&[]),
            Err(StoreError::Corrupt(_))
        ));
    }

    #[test]
    fn undo_row_decode_truncated_is_corrupt() {
        let r = UndoRow {
            created_boxes: vec![[1; 32]],
            spent_boxes: vec![[2; 32]],
            tx_ids: vec![[3; 32]],
            tree_txs: vec![([9; 32], 8)],
            prev_balances: vec![(
                [4; 32],
                Some(BalanceRow {
                    nano: 1,
                    tokens: vec![],
                    box_count: 1,
                    first_seen: 1,
                    last_seen: 2,
                }),
            )],
            prev_next_box_gidx: 5,
            prev_next_tx_gidx: 6,
            new_trees: vec![[7; 32]],
        };
        let full = r.encode();
        assert!(matches!(
            UndoRow::decode(&full[..full.len() - 1]),
            Err(StoreError::Corrupt(_))
        ));
        assert!(matches!(UndoRow::decode(&[]), Err(StoreError::Corrupt(_))));
    }
}
