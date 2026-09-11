pub const RENT_PERIOD: u32 = 1_051_200;
pub const RENT_PER_BYTE: u64 = 1_250_000;

/// Largest box, in serialized bytes, whose consensus storage fee is still positive.
///
/// Consensus computes the fee as a **signed 32-bit** multiply (matching the reference
/// implementation), so `size * RENT_PER_BYTE` wraps negative from 1,718 bytes upward. A box at
/// or above that size cannot be rent-claimed at all: the recreate rule would require the
/// claimer to *add* value rather than take it.
pub const MAX_CLAIMABLE_SIZE_BYTES: u32 = 1_717;

pub fn maturity_height(creation: u32) -> u32 {
    creation.saturating_add(RENT_PERIOD)
}

/// Nominal rent for a box of `size_bytes`, capped by its value.
///
/// This is the textbook figure and is what a reader expects to see, but it is **not** proof of
/// a collectible opportunity: see [`consensus_storage_fee`] and [`is_rent_claimable`]. Only
/// meaningful when the fee is positive under consensus.
pub fn rent_due(size_bytes: u32, value: u64) -> u64 {
    (size_bytes as u64).saturating_mul(RENT_PER_BYTE).min(value)
}

/// The storage fee exactly as consensus computes it: `size * RENT_PER_BYTE` in **wrapping
/// i32** arithmetic. Negative for any box of [`MAX_CLAIMABLE_SIZE_BYTES`] + 1 bytes or more.
pub fn consensus_storage_fee(size_bytes: u32) -> i32 {
    (size_bytes as i32).wrapping_mul(RENT_PER_BYTE as i32)
}

/// Whether a box can be rent-claimed at all, i.e. its consensus storage fee is positive.
pub fn is_rent_claimable(size_bytes: u32) -> bool {
    consensus_storage_fee(size_bytes) > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consensus_constants() {
        // Known mainnet recreate-claim: box created at 814_800 became claimable at 1_866_000.
        assert_eq!(maturity_height(814_800), 1_866_000);
        assert_eq!(RENT_PERIOD, 1_051_200);
        assert_eq!(RENT_PER_BYTE, 1_250_000);
    }
    #[test]
    fn rent_is_capped_by_value() {
        assert_eq!(rent_due(105, 1_000_000_000), 131_250_000);
        assert_eq!(rent_due(105, 1_000_000), 1_000_000);
        assert_eq!(
            rent_due(u32::MAX, u64::MAX),
            (u32::MAX as u64) * RENT_PER_BYTE
        );
    }

    #[test]
    fn the_consensus_fee_goes_negative_above_1717_bytes() {
        // The boundary: 1,717 bytes still fits in i32, 1,718 wraps.
        assert_eq!(consensus_storage_fee(1_717), 2_146_250_000);
        assert!(consensus_storage_fee(1_718) < 0);
        assert_eq!(MAX_CLAIMABLE_SIZE_BYTES, 1_717);
        assert!(is_rent_claimable(1_717));
        assert!(!is_rent_claimable(1_718));
        // A real example from the live eligible list: 2,008 bytes.
        assert_eq!(consensus_storage_fee(2_008), -1_784_967_296);
        assert!(!is_rent_claimable(2_008));
    }

    #[test]
    fn nominal_rent_looks_collectible_when_it_is_not() {
        // This is exactly the trap: `rent_due` reports a healthy figure for a box whose
        // consensus fee is negative, so the two must always be read together.
        assert_eq!(rent_due(2_008, 10_000_000_000), 2_510_000_000);
        assert!(!is_rent_claimable(2_008));
    }
}
