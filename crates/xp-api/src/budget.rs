//! Request-local safety net for legacy transaction expansion. See docs/operations/transaction-budgets.md.
use crate::dto::{box_dto_budgeted, parse_id, InputDto, TxDto};
use crate::ApiError;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};
use xp_store::read::ExpansionRow;
use xp_store::rows::TxRow;
use xp_store::Reader;
use xp_types::{hex32, Hash32};

pub(crate) const MAX_WORK: usize = 10_000;
pub(crate) const MAX_BYTES: usize = 2 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(4);

pub(crate) fn limit(code: &'static str) -> ApiError {
    ApiError::Expansion(code)
}

pub(crate) struct Budget {
    work: usize,
    decoded_bytes: usize,
    deadline: Instant,
}
impl Budget {
    pub(crate) fn new() -> Self {
        Self {
            work: 0,
            decoded_bytes: 0,
            deadline: Instant::now() + DEADLINE,
        }
    }
    fn check_at(&self, now: Instant) -> Result<(), ApiError> {
        if now > self.deadline {
            return Err(limit("expansion_deadline"));
        }
        Ok(())
    }
    pub(crate) fn check(&self) -> Result<(), ApiError> {
        self.check_at(Instant::now())
    }
    fn check_store(&self) -> Result<(), xp_store::StoreError> {
        self.check()
            .map_err(|_| xp_store::StoreError::ReadLimit("expansion_deadline"))
    }
    pub(crate) fn work(&mut self, units: usize) -> Result<(), ApiError> {
        self.check()?;
        self.work = self
            .work
            .checked_add(units)
            .filter(|n| *n <= MAX_WORK)
            .ok_or_else(|| limit("expansion_work_limit"))?;
        Ok(())
    }
    fn admit_row(&mut self, rd: &Reader, kind: ExpansionRow, id: &Hash32) -> Result<(), ApiError> {
        self.work(1)?;
        let bytes = rd.expansion_row_len(kind, id)?;
        self.admit_bytes(bytes)
    }
    pub(crate) fn admit_tx_row(&mut self, bytes: usize) -> Result<(), xp_store::StoreError> {
        self.admit_bytes(bytes).map_err(|error| match error {
            ApiError::Expansion(code) => xp_store::StoreError::ReadLimit(code),
            _ => unreachable!("admission only returns resource limits"),
        })
    }
    fn admit_bytes(&mut self, bytes: usize) -> Result<(), ApiError> {
        self.check()?;
        self.decoded_bytes = self
            .decoded_bytes
            .checked_add(bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(|| limit("expansion_decode_limit"))?;
        Ok(())
    }
    fn box_dto(
        &mut self,
        rd: &Reader,
        id: &Hash32,
        owner: Option<&Hash32>,
        tip: Option<u32>,
        emission: Option<&Hash32>,
    ) -> Result<Option<crate::dto::BoxDto>, ApiError> {
        self.admit_row(rd, ExpansionRow::Box, id)?;
        let Some(row) = rd.box_by_id_checked(id, || self.check_store())? else {
            return if owner.is_none() && rd.partial_from()?.is_some() {
                Ok(None)
            } else {
                Err(ApiError::Integrity("missing transaction box".into()))
            };
        };
        if owner.is_some_and(|owner| *owner != row.tx_id) {
            return Err(ApiError::Integrity(
                "output belongs to another transaction".into(),
            ));
        }
        self.admit_row(rd, ExpansionRow::Tree, &row.tree_hash)?;
        let tree = rd.tree_row_checked(&row.tree_hash, || self.check_store())?;
        // Charge all token and register/tree byte work before DTO vectors, hex strings or JSON parsing.
        self.work(row.tokens.len())?;
        self.work(row.registers_json.len().div_ceil(128))?;
        if let Some(tree) = &tree {
            self.work((tree.tree_bytes.len() + tree.address.len()).div_ceil(128))?;
        }
        let mut dto = box_dto_budgeted(id, &row, tree.as_ref(), tip, emission, Some(self))?;
        for token in &mut dto.tokens {
            let id = parse_id(&token.id)?;
            self.admit_row(rd, ExpansionRow::Token, &id)?;
            // A single token per batch keeps both decoding and cloning behind admission.
            if let Some((_, name, decimals)) =
                rd.token_names_checked(&[id], || self.check_store())?.pop()
            {
                self.work(name.len().div_ceil(128))?;
                token.name = Some(name);
                token.decimals = decimals;
            }
        }
        self.check()?;
        Ok(Some(dto))
    }
    pub(crate) fn tx(
        &mut self,
        rd: &Reader,
        id: &Hash32,
        row: &TxRow,
        tip: Option<u32>,
        emission: Option<&Hash32>,
    ) -> Result<TxDto, ApiError> {
        // Reject one oversized transaction before reserving its input/output vectors.
        self.work(1 + row.inputs.len() + row.data_inputs.len() + usize::from(row.output_count))?;
        let mut inputs = Vec::new();
        for id in &row.inputs {
            inputs.push(InputDto {
                id: hex32(id),
                box_: self.box_dto(rd, id, None, tip, emission)?,
            });
        }
        let mut outputs = Vec::new();
        for index in 0..row.output_count {
            self.work(1)?;
            let box_id = rd.output_id(row, index)?;
            outputs.push(
                self.box_dto(rd, &box_id, Some(id), tip, emission)?
                    .ok_or_else(|| ApiError::Integrity("missing output".into()))?,
            );
        }
        let mut data_inputs = Vec::new();
        for id in &row.data_inputs {
            self.check()?;
            data_inputs.push(hex32(id));
        }
        Ok(TxDto {
            id: hex32(id),
            height: row.height,
            index: row.index,
            timestamp: row.timestamp,
            size: row.size,
            fee: row.fee.to_string(),
            inputs,
            data_inputs,
            outputs,
        })
    }
    pub(crate) fn registers(&self, json: &str) -> Result<serde_json::Value, ApiError> {
        let parsed = serde_json::from_reader(CheckedRead {
            budget: self,
            bytes: json.as_bytes(),
        });
        self.check()?;
        parsed.map_err(|_| ApiError::Integrity("invalid stored register JSON".into()))
    }
    pub(crate) fn hex(&self, bytes: &[u8]) -> Result<String, ApiError> {
        let mut result = String::new();
        for chunk in bytes.chunks(128) {
            self.check()?;
            result.push_str(&hex::encode(chunk));
        }
        Ok(result)
    }
    /// Serialize on the blocking worker while it still owns its permit. The writer checks
    /// BEFORE extending, including JSON escapes and the surrounding page/array punctuation.
    pub(crate) fn json(&self, value: &impl Serialize) -> Result<Response, ApiError> {
        let mut writer = JsonWriter {
            budget: self,
            bytes: Vec::new(),
            failure: None,
        };
        if let Err(error) = serde_json::to_writer(&mut writer, value) {
            return Err(writer
                .failure
                .unwrap_or_else(|| ApiError::Internal(error.to_string())));
        }
        self.check()?;
        Ok(([(header::CONTENT_TYPE, "application/json")], writer.bytes).into_response())
    }
}
// serde_json's reader parser asks for bytes incrementally, so even one large register
// string cannot hide an unchecked parsing loop behind a single from_str call.
struct CheckedRead<'a> {
    budget: &'a Budget,
    bytes: &'a [u8],
}
impl Read for CheckedRead<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.budget
            .check()
            .map_err(|_| io::Error::other("expansion deadline"))?;
        let count = out.len().min(128);
        self.bytes.read(&mut out[..count])
    }
}

struct JsonWriter<'a> {
    budget: &'a Budget,
    bytes: Vec<u8>,
    failure: Option<ApiError>,
}
impl Write for JsonWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let result = self.budget.check().and_then(|()| {
            if buf.len() > MAX_BYTES - self.bytes.len() {
                Err(limit("expansion_response_limit"))
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            self.failure = Some(error);
            return Err(io::Error::other("expansion limit"));
        }
        for chunk in buf.chunks(128) {
            if let Err(error) = self.budget.check() {
                self.failure = Some(error);
                return Err(io::Error::other("expansion deadline"));
            }
            self.bytes.extend_from_slice(chunk);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transaction_slots_are_cumulative_and_admitted_before_expansion() {
        let dir = tempfile::tempdir().unwrap();
        let store = xp_store::Store::open(&dir.path().join("budget.redb")).unwrap();
        let rd = Reader::new(&store).unwrap();
        let mut row = TxRow {
            height: 1,
            index: 0,
            gidx: 0,
            first_out_gidx: 0,
            timestamp: 1,
            size: 1,
            fee: 0,
            inputs: vec![],
            data_inputs: vec![[0; 32]; MAX_WORK - 1],
            output_count: 0,
        };
        let mut budget = Budget::new();
        budget.tx(&rd, &[0; 32], &row, None, None).unwrap();
        assert_eq!(budget.work, MAX_WORK);
        row.data_inputs.clear();
        assert!(matches!(
            budget.tx(&rd, &[0; 32], &row, None, None),
            Err(ApiError::Expansion("expansion_work_limit"))
        ));
        for outputs in [false, true] {
            let mut budget = Budget::new();
            if outputs {
                row.output_count = MAX_WORK as u16;
                row.inputs.clear();
            } else {
                row.inputs = vec![[0; 32]; MAX_WORK];
            }
            assert!(matches!(
                budget.tx(&rd, &[0; 32], &row, None, None),
                Err(ApiError::Expansion("expansion_work_limit"))
            ));
            assert_eq!(store.lookup_counts(), [0; 3]);
        }
    }

    #[test]
    fn real_token_register_expansion_at_work_threshold_and_one_over() {
        let dir = tempfile::tempdir().unwrap();
        let store = xp_store::Store::open(&dir.path().join("budget.redb")).unwrap();
        let block = xp_wire::decode_block(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/blocks/1866000.json"
            ))
            .unwrap(),
        )
        .unwrap();
        store
            .seed_for_tests(1865999, block.header.parent_id.0)
            .unwrap();
        store.apply_batch(&[block], true).unwrap();
        let rd = Reader::new(&store).unwrap();
        let rows = rd.txs_in_block(1866000).unwrap();
        let mut saw_tokens = false;
        let mut saw_registers = false;
        for (id, row) in rows {
            let mut baseline = Budget::new();
            let dto = baseline.tx(&rd, &id, &row, Some(1866000), None).unwrap();
            saw_tokens |= dto.outputs.iter().any(|b| !b.tokens.is_empty());
            saw_registers |= dto
                .outputs
                .iter()
                .any(|b| b.registers.as_object().is_some_and(|r| !r.is_empty()));
            let cost = baseline.work;
            let mut at = Budget::new();
            at.work(MAX_WORK - cost).unwrap();
            at.tx(&rd, &id, &row, Some(1866000), None).unwrap();
            assert_eq!(at.work, MAX_WORK);
            let mut over = Budget::new();
            over.work(MAX_WORK - cost + 1).unwrap();
            assert!(matches!(
                over.tx(&rd, &id, &row, Some(1866000), None),
                Err(ApiError::Expansion("expansion_work_limit"))
            ));
        }
        assert!(saw_tokens && saw_registers);
    }

    #[test]
    fn expired_budget_stops_register_parsing_hex_and_serialization() {
        let mut budget = Budget::new();
        budget.deadline = Instant::now() - Duration::from_nanos(1);
        assert!(matches!(
            budget.registers("{}"),
            Err(ApiError::Expansion("expansion_deadline"))
        ));
        assert!(matches!(
            budget.hex(&[0; 129]),
            Err(ApiError::Expansion("expansion_deadline"))
        ));
        assert!(matches!(
            budget.json(&vec![0; 129]),
            Err(ApiError::Expansion("expansion_deadline"))
        ));
    }

    #[test]
    fn work_decode_and_deadline_boundaries() {
        let mut budget = Budget::new();
        budget.work(MAX_WORK).unwrap();
        assert!(matches!(
            budget.work(1),
            Err(ApiError::Expansion("expansion_work_limit"))
        ));
        let mut budget = Budget::new();
        budget.admit_bytes(MAX_BYTES).unwrap();
        assert!(matches!(
            budget.admit_bytes(1),
            Err(ApiError::Expansion("expansion_decode_limit"))
        ));
        assert!(budget.check_at(budget.deadline).is_ok());
        assert!(matches!(
            budget.check_at(budget.deadline + Duration::from_nanos(1)),
            Err(ApiError::Expansion("expansion_deadline"))
        ));
    }
    #[test]
    fn exact_json_bytes_and_one_over_including_escapes() {
        let budget = Budget::new();
        budget.json(&"x".repeat(MAX_BYTES - 2)).unwrap();
        assert!(matches!(
            budget.json(&"x".repeat(MAX_BYTES - 1)),
            Err(ApiError::Expansion("expansion_response_limit"))
        ));
        budget.json(&"\n".repeat((MAX_BYTES - 2) / 2)).unwrap();
        assert!(matches!(
            budget.json(&"\n".repeat(MAX_BYTES / 2)),
            Err(ApiError::Expansion("expansion_response_limit"))
        ));
    }
}
