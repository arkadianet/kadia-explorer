pub const RENT_PERIOD: u32 = 1_051_200;
pub const RENT_PER_BYTE: u64 = 1_250_000;

pub fn maturity_height(creation: u32) -> u32 {
    creation.saturating_add(RENT_PERIOD)
}

pub fn rent_due(size_bytes: u32, value: u64) -> u64 {
    (size_bytes as u64).saturating_mul(RENT_PER_BYTE).min(value)
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
}
