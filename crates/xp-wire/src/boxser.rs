//! Our own `ErgoBox.bytes` serialiser.
//!
//! ergo-lib's JSON `ErgoBox` deserialiser re-serialises the tree canonically and rejects the
//! box when the recomputed id differs from the node's. Boxes with non-canonical (but chain
//! accepted) encodings therefore made whole blocks undecodable. The node's `boxId` is
//! authoritative, so we serialise from the JSON verbatim instead, and only use the result for
//! the consensus box size — proven correct by `blake2b256(bytes) == boxId`.
//!
//! Layout (sigmastate `ErgoBox.sigmaSerializer`):
//! VLQ(value) ‖ ergoTree bytes ‖ VLQ(creationHeight) ‖ VLQ(#tokens) ‖
//! [32-byte tokenId ‖ VLQ(amount)]* ‖ u8(#registers) ‖ [register bytes in R4..R9 order]* ‖
//! 32-byte transactionId ‖ VLQ(index)

use xp_types::Hash32;

use crate::WireError;

/// Register names in serialisation order. Registers are contiguous from R4 upwards.
pub const REGISTER_NAMES: [&str; 6] = ["R4", "R5", "R6", "R7", "R8", "R9"];

/// Unsigned VLQ (LEB128): 7 bits per byte, little-endian, continuation bit `0x80`.
pub fn put_vlq(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// The pieces of a box, taken verbatim from the node's JSON.
pub struct BoxParts<'a> {
    pub value: u64,
    pub tree_bytes: &'a [u8],
    pub creation_height: u32,
    pub tokens: &'a [(Hash32, u64)],
    /// Raw serialised register constants, in R4..R9 order.
    pub registers: &'a [Vec<u8>],
    pub tx_id: &'a Hash32,
    pub index: u16,
}

/// `ErgoBox.bytes` — the consensus storage-rent size basis.
pub fn box_bytes(p: &BoxParts<'_>) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + p.tree_bytes.len());
    put_vlq(&mut out, p.value);
    out.extend_from_slice(p.tree_bytes);
    put_vlq(&mut out, u64::from(p.creation_height));
    put_vlq(&mut out, p.tokens.len() as u64);
    for (id, amount) in p.tokens {
        out.extend_from_slice(id);
        put_vlq(&mut out, *amount);
    }
    out.push(p.registers.len() as u8);
    for r in p.registers {
        out.extend_from_slice(r);
    }
    out.extend_from_slice(p.tx_id);
    put_vlq(&mut out, u64::from(p.index));
    out
}

pub(crate) fn hex_field(v: &serde_json::Value, field: &'static str) -> Result<Vec<u8>, WireError> {
    let s = v
        .get(field)
        .and_then(|x| x.as_str())
        .ok_or(WireError::MissingField(field))?;
    hex::decode(s).map_err(|_| WireError::MissingField(field))
}

pub(crate) fn hash32_field(
    v: &serde_json::Value,
    field: &'static str,
) -> Result<Hash32, WireError> {
    let bytes = hex_field(v, field)?;
    let out: Hash32 = bytes
        .try_into()
        .map_err(|_| WireError::MissingField(field))?;
    Ok(out)
}

pub(crate) fn u64_field(v: &serde_json::Value, field: &'static str) -> Result<u64, WireError> {
    v.get(field)
        .and_then(|x| x.as_u64())
        .ok_or(WireError::MissingField(field))
}

/// Guards the narrowing `as` casts: a value the node could not really have produced turns
/// into an error rather than silently wrapping around.
pub(crate) fn bounded(n: u64, max: u64, name: &'static str) -> Result<u64, WireError> {
    if n > max {
        return Err(WireError::OutOfRange(name));
    }
    Ok(n)
}

pub(crate) fn tokens_of(v: &serde_json::Value) -> Result<Vec<(Hash32, u64)>, WireError> {
    let Some(assets) = v.get("assets") else {
        return Ok(Vec::new());
    };
    let assets = assets.as_array().ok_or(WireError::MissingField("assets"))?;
    assets
        .iter()
        .map(|a| Ok((hash32_field(a, "tokenId")?, u64_field(a, "amount")?)))
        .collect()
}

/// Raw register bytes in R4..R9 order. Registers are contiguous from R4 up, so an absent one
/// ends the run; a register that is present but not a hex string is an error, not a stop.
pub(crate) fn registers_from_object(
    regs: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<Vec<u8>>, WireError> {
    let mut out = Vec::new();
    for name in REGISTER_NAMES {
        let Some(val) = regs.get(name) else {
            break;
        };
        let hex_str = val
            .as_str()
            .ok_or(WireError::MissingField("additionalRegisters"))?;
        out.push(hex::decode(hex_str).map_err(|_| WireError::MissingField("additionalRegisters"))?);
    }
    Ok(out)
}

pub(crate) fn registers_of(v: &serde_json::Value) -> Result<Vec<Vec<u8>>, WireError> {
    let Some(regs) = v.get("additionalRegisters") else {
        return Ok(Vec::new());
    };
    let regs = regs
        .as_object()
        .ok_or(WireError::MissingField("additionalRegisters"))?;
    registers_from_object(regs)
}

/// Serialise a box straight from a node JSON object (`transactionId`/`index` read from it).
pub fn box_bytes_from_json(v: &serde_json::Value) -> Result<Vec<u8>, WireError> {
    let tx_id = hash32_field(v, "transactionId")?;
    let index = bounded(u64_field(v, "index")?, u64::from(u16::MAX), "box.index")? as u16;
    Ok(box_bytes(&BoxParts {
        value: u64_field(v, "value")?,
        tree_bytes: &hex_field(v, "ergoTree")?,
        creation_height: bounded(
            u64_field(v, "creationHeight")?,
            u64::from(u32::MAX),
            "box.creationHeight",
        )? as u32,
        tokens: &tokens_of(v)?,
        registers: &registers_of(v)?,
        tx_id: &tx_id,
        index,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vlq(v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        put_vlq(&mut out, v);
        out
    }

    #[test]
    fn vlq_encodes_boundaries() {
        assert_eq!(vlq(0), vec![0x00]);
        assert_eq!(vlq(1), vec![0x01]);
        assert_eq!(vlq(127), vec![0x7f]);
        assert_eq!(vlq(128), vec![0x80, 0x01]);
        assert_eq!(vlq(16383), vec![0xff, 0x7f]);
        assert_eq!(vlq(16384), vec![0x80, 0x80, 0x01]);
        assert_eq!(
            vlq(u64::MAX),
            vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]
        );
    }

    #[test]
    fn vlq_round_trips_through_a_reference_decoder() {
        fn decode(bytes: &[u8]) -> u64 {
            let mut v = 0u64;
            for (i, b) in bytes.iter().enumerate() {
                v |= u64::from(b & 0x7f) << (7 * i);
            }
            v
        }
        for n in [
            0,
            1,
            127,
            128,
            16383,
            16384,
            1 << 40,
            u64::MAX - 1,
            u64::MAX,
        ] {
            assert_eq!(decode(&vlq(n)), n, "vlq round trip for {n}");
        }
    }
}
