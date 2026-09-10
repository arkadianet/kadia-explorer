pub mod rent;

pub type Hash32 = [u8; 32];
pub type Gidx = u64;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub Hash32);
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), hex32(&self.0))
            }
        }
        impl std::str::FromStr for $name {
            type Err = TypesError;
            fn from_str(s: &str) -> Result<Self, TypesError> {
                parse_hex32(s).map($name)
            }
        }
    };
}
id_newtype!(BoxId);
id_newtype!(TxId);
id_newtype!(HeaderId);
id_newtype!(TreeHash);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TypesError {
    #[error("bad 32-byte hex: {0}")]
    BadHex(String),
}

pub fn parse_hex32(s: &str) -> Result<Hash32, TypesError> {
    let v = hex::decode(s).map_err(|_| TypesError::BadHex(s.to_owned()))?;
    <Hash32>::try_from(v).map_err(|_| TypesError::BadHex(s.to_owned()))
}

pub fn hex32(h: &Hash32) -> String {
    hex::encode(h)
}

/// Mainnet genesis allocation, in nanoERG, from the three chain-spec boxes served by
/// `/utxo/genesis`. Verified against a live node on 2026-09-10.
///
/// The emission contract pays the miner (and, in the first years, the foundation) out of
/// `GENESIS_EMISSION_NANO` block by block, so the ERG that actually exists at any height is
/// `GENESIS_TOTAL_NANO` minus whatever the emission contract still holds. Deriving it that way
/// is exact and needs no emission schedule — which matters because the hardcoded pre-EIP-27
/// schedule this replaced was wrong: it assumed 75 ERG/block where the chain paid 67.5 (10%
/// went to the foundation), and ignored EIP-27 re-emission entirely.
pub const GENESIS_EMISSION_NANO: u64 = 93_409_132_500_000_000;
/// Foundation box at genesis (`4,330,791.5 ERG`).
pub const GENESIS_FOUNDATION_NANO: u64 = 4_330_791_500_000_000;
/// The "no premine" proof box at genesis (`1 ERG`).
pub const GENESIS_NO_PREMINE_NANO: u64 = 1_000_000_000;
/// Every nanoERG the chain started with: 97,739,925 ERG.
pub const GENESIS_TOTAL_NANO: u64 =
    GENESIS_EMISSION_NANO + GENESIS_FOUNDATION_NANO + GENESIS_NO_PREMINE_NANO;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_roundtrip() {
        let s = "aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6";
        assert_eq!(hex32(&parse_hex32(s).unwrap()), s);
        assert_eq!(parse_hex32("zz"), Err(TypesError::BadHex("zz".into())));
        assert_eq!(parse_hex32("aabb"), Err(TypesError::BadHex("aabb".into())));
    }
}
