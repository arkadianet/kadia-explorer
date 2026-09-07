//! EIP-4 token metadata: decoding `Coll[Byte]` register constants and reading the fixed
//! R4..R7 fields (name / description / decimals / type), plus the token mint rule.
//!
//! A `Coll[Byte]` sigma constant is serialised as `0e` (type code) ‖ `VLQ(len)` ‖ `bytes`,
//! with no trailing data — anything else (wrong type code, truncated length, leftover bytes)
//! is not a valid `Coll[Byte]` and yields `None` rather than a partial value.

use crate::boxser::get_vlq;
use crate::DecodedBox;
use xp_types::Hash32;

const COLL_BYTE_TYPE_CODE: u8 = 0x0e;

/// Decodes a `Coll[Byte]` sigma constant (`"0e" + VLQ(len) + bytes`) from its hex encoding.
///
/// Requires the type code, a valid VLQ length, exactly that many bytes to follow, and no
/// bytes left over — any other shape (wrong type code, short/garbled input, trailing bytes)
/// returns `None`.
pub fn coll_byte(hex: &str) -> Option<Vec<u8>> {
    let bytes = hex::decode(hex).ok()?;
    let (&type_code, rest) = bytes.split_first()?;
    if type_code != COLL_BYTE_TYPE_CODE {
        return None;
    }
    let (len, rest) = get_vlq(rest)?;
    let len = usize::try_from(len).ok()?;
    if rest.len() != len {
        return None;
    }
    Some(rest.to_vec())
}

/// [`coll_byte`] followed by UTF-8 decoding; invalid UTF-8 (or a non-`Coll[Byte]` input)
/// yields `None` rather than lossily replacing bytes.
pub fn coll_byte_utf8(hex: &str) -> Option<String> {
    String::from_utf8(coll_byte(hex)?).ok()
}

/// EIP-4 token metadata read from a box's registers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Eip4 {
    pub name: String,
    pub description: String,
    pub decimals: Option<u8>,
    /// Raw hex of R7's `Coll[Byte]` payload (e.g. `"0101"`); frontend maps well-known values
    /// to a token kind and falls back to "other".
    pub token_type: Option<String>,
}

/// Parses EIP-4 metadata out of a box's `registers_json` (the node's `{"R4": "<hex>", ...}`
/// object, verbatim). Missing or malformed registers leave the corresponding field at its
/// default (`""` for name/description, `None` for decimals/token_type).
pub fn parse_eip4(registers_json: &str) -> Eip4 {
    let Ok(serde_json::Value::Object(regs)) = serde_json::from_str(registers_json) else {
        return Eip4::default();
    };
    let reg_hex = |name: &str| regs.get(name).and_then(|v| v.as_str());

    let name = reg_hex("R4").and_then(coll_byte_utf8).unwrap_or_default();
    let description = reg_hex("R5").and_then(coll_byte_utf8).unwrap_or_default();
    let decimals = reg_hex("R6")
        .and_then(coll_byte_utf8)
        .and_then(|s| s.parse::<u8>().ok());
    let token_type = reg_hex("R7").and_then(coll_byte).map(hex::encode);

    Eip4 {
        name,
        description,
        decimals,
        token_type,
    }
}

/// The token minted by a transaction, if any: per Ergo consensus, a token whose id equals
/// the id of the transaction's first input box is minted in that transaction, with the
/// minted amount being the total of that token across the transaction's outputs.
pub fn minted_token_of(first_input: &Hash32, outputs: &[DecodedBox]) -> Option<(Hash32, u64)> {
    let total: u64 = outputs
        .iter()
        .flat_map(|o| o.tokens.iter())
        .filter(|(id, _)| id == first_input)
        .map(|(_, amount)| amount)
        .sum();
    if total == 0 {
        return None;
    }
    Some((*first_input, total))
}
