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
